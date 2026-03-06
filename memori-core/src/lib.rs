mod graph_extractor;
mod llm_generator;

use std::path::PathBuf;
use std::sync::Arc;

use graph_extractor::{GraphData, extract_entities};
use llm_generator::generate_answer as generate_llm_answer;
pub use memori_parser::DocumentChunk;
use memori_parser::{ParserStub, parse_and_chunk};
use memori_storage::{SqliteStore, StorageError, VectorStore};
use memori_vault::{
    MemoriVaultConfig, MemoriVaultError, MemoriVaultHandle, WatchEvent, WatchEventKind,
    create_event_channel, spawn_memori_vault,
};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

const DEFAULT_OLLAMA_BASE_URL: &str = "http://localhost:11434";
const DEFAULT_OLLAMA_EMBED_MODEL: &str = "nomic-embed-text";
const DEFAULT_DB_FILE_NAME: &str = ".memori.db";

/// 前端/CLI 可消费的 Vault 统计信息。
#[derive(Debug, Clone, serde::Serialize)]
pub struct VaultStats {
    pub document_count: u64,
    pub chunk_count: u64,
    pub graph_node_count: u64,
}

/// 全局共享状态。
/// 当前持有 parser 占位、SQLite 持久化存储与本地 Ollama 客户端。
#[derive(Debug)]
pub struct AppState {
    pub parser: ParserStub,
    pub vector_store: Arc<SqliteStore>,
    pub ollama_client: OllamaEmbeddingClient,
}

impl AppState {
    pub fn new(db_path: impl Into<PathBuf>) -> Result<Self, StorageError> {
        let vector_store = Arc::new(SqliteStore::new(db_path.into())?);
        Ok(Self {
            parser: ParserStub,
            vector_store,
            ollama_client: OllamaEmbeddingClient::default(),
        })
    }
}

/// 极简 Ollama Embedding 客户端。
#[derive(Debug, Clone)]
pub struct OllamaEmbeddingClient {
    http: reqwest::Client,
    base_url: String,
    model: String,
}

impl Default for OllamaEmbeddingClient {
    fn default() -> Self {
        Self::new(DEFAULT_OLLAMA_BASE_URL, DEFAULT_OLLAMA_EMBED_MODEL)
    }
}

impl OllamaEmbeddingClient {
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.into(),
            model: model.into(),
        }
    }

    pub async fn embed_text(&self, prompt: &str) -> Result<Vec<f32>, OllamaClientError> {
        let url = format!("{}/api/embeddings", self.base_url.trim_end_matches('/'));

        let response = self
            .http
            .post(url)
            .json(&OllamaEmbeddingRequest {
                model: &self.model,
                prompt,
            })
            .send()
            .await
            .map_err(OllamaClientError::Request)?;

        let status = response.status();
        if !status.is_success() {
            let body = match response.text().await {
                Ok(text) => text,
                Err(err) => format!("<读取响应体失败: {err}>"),
            };

            return Err(OllamaClientError::HttpStatus {
                status: status.as_u16(),
                body,
            });
        }

        let parsed: OllamaEmbeddingResponse =
            response.json().await.map_err(OllamaClientError::Request)?;

        if parsed.embedding.is_empty() {
            return Err(OllamaClientError::EmptyEmbedding);
        }

        Ok(parsed.embedding)
    }
}

#[derive(Debug, serde::Serialize)]
struct OllamaEmbeddingRequest<'a> {
    model: &'a str,
    prompt: &'a str,
}

#[derive(Debug, serde::Deserialize)]
struct OllamaEmbeddingResponse {
    embedding: Vec<f32>,
}

#[derive(Debug, Error)]
pub enum OllamaClientError {
    #[error("Embedding 请求失败: {0}")]
    Request(#[source] reqwest::Error),

    #[error("Ollama 返回非成功状态: {status}, body: {body}")]
    HttpStatus { status: u16, body: String },

    #[error("Ollama 返回空向量")]
    EmptyEmbedding,
}

/// memori-core 统一错误定义。
#[derive(Debug, Error)]
pub enum EngineError {
    #[error("Memori-Vault 组件错误: {0}")]
    MemoriVault(#[from] MemoriVaultError),

    #[error("存储层错误: {0}")]
    Storage(#[from] StorageError),

    #[error("本地大模型请求错误: {0}")]
    Ollama(#[from] OllamaClientError),

    #[error("图谱抽取请求失败: {0}")]
    GraphExtractRequest(#[source] reqwest::Error),

    #[error("图谱抽取接口返回非成功状态: {status}, body: {body}")]
    GraphExtractHttpStatus { status: u16, body: String },

    #[error("图谱抽取响应反序列化失败: {0}")]
    GraphExtractDeserialize(#[source] reqwest::Error),

    #[error("图谱抽取 JSON 解析失败: {source}; 原始内容: {raw}")]
    GraphExtractJson {
        raw: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("答案生成请求失败: {0}")]
    AnswerGenerateRequest(#[source] reqwest::Error),

    #[error("答案生成接口返回非成功状态: {status}, body: {body}")]
    AnswerGenerateHttpStatus { status: u16, body: String },

    #[error("答案生成响应反序列化失败: {0}")]
    AnswerGenerateDeserialize(#[source] reqwest::Error),

    #[error("答案生成响应为空")]
    AnswerGenerateEmpty,

    #[error("获取当前工作目录失败: {0}")]
    CurrentDir(#[source] std::io::Error),

    #[error("守护任务已启动，禁止重复启动")]
    DaemonAlreadyStarted,

    #[error("事件接收通道不可用")]
    EventChannelUnavailable,

    #[error("核心守护任务 Join 失败: {0}")]
    DaemonTaskJoin(#[from] tokio::task::JoinError),
}

/// MemoriEngine：核心中枢。
/// - 持有全局共享状态 Arc<AppState>
/// - 持有文件事件接收通道
/// - 负责启动并管理异步消费守护任务
pub struct MemoriEngine {
    state: Arc<AppState>,
    event_rx: Option<mpsc::Receiver<WatchEvent>>,
    daemon_task: Option<JoinHandle<Result<(), EngineError>>>,
    memori_vault_handle: Option<MemoriVaultHandle>,
}

impl MemoriEngine {
    /// 用现成 receiver 构造引擎（便于测试和外部注入）。
    pub fn new(state: Arc<AppState>, event_rx: mpsc::Receiver<WatchEvent>) -> Self {
        Self {
            state,
            event_rx: Some(event_rx),
            daemon_task: None,
            memori_vault_handle: None,
        }
    }

    /// 快速引导：创建事件通道 + 启动 memori-vault 监听端 + 初始化 SQLite 存储。
    pub fn bootstrap(root: impl Into<PathBuf>) -> Result<Self, EngineError> {
        let config = MemoriVaultConfig::new(root);
        Self::bootstrap_with_config(config)
    }

    /// 通过配置引导引擎。
    pub fn bootstrap_with_config(config: MemoriVaultConfig) -> Result<Self, EngineError> {
        let (event_tx, event_rx) = create_event_channel();
        let memori_vault_handle = spawn_memori_vault(config, event_tx)?;

        let db_path = std::env::current_dir()
            .map_err(EngineError::CurrentDir)?
            .join(DEFAULT_DB_FILE_NAME);

        let state = Arc::new(AppState::new(db_path)?);
        let mut engine = Self::new(state, event_rx);
        engine.memori_vault_handle = Some(memori_vault_handle);
        Ok(engine)
    }

    /// 读取共享状态句柄（供外部组件访问）。
    pub fn state(&self) -> Arc<AppState> {
        Arc::clone(&self.state)
    }

    /// 语义检索 API：
    /// 1) 先将 query 向量化；
    /// 2) 在向量存储中检索 top-k 相似块。
    pub async fn search(
        &self,
        query: &str,
        top_k: usize,
    ) -> Result<Vec<(DocumentChunk, f32)>, EngineError> {
        if query.trim().is_empty() || top_k == 0 {
            return Ok(Vec::new());
        }

        let query_embedding = self.state.ollama_client.embed_text(query).await?;
        let results = self
            .state
            .vector_store
            .search_similar(query_embedding, top_k)
            .await?;

        Ok(results)
    }

    /// 根据检索结果对应的 chunk_id，拉取 1-hop 图谱上下文。
    pub async fn get_graph_context_for_results(
        &self,
        results: &[(DocumentChunk, f32)],
    ) -> Result<String, EngineError> {
        if results.is_empty() {
            return Ok(String::new());
        }

        let mut chunk_ids = Vec::new();
        for (chunk, _score) in results {
            match self
                .state
                .vector_store
                .resolve_chunk_id(&chunk.file_path, chunk.chunk_index)
                .await?
            {
                Some(chunk_id) => chunk_ids.push(chunk_id),
                None => {
                    warn!(
                        path = %chunk.file_path.display(),
                        chunk_index = chunk.chunk_index,
                        "未能从检索结果反查 chunk_id，已跳过该条图谱上下文"
                    );
                }
            }
        }

        chunk_ids.sort_unstable();
        chunk_ids.dedup();

        let graph_context = self
            .state
            .vector_store
            .get_graph_context_for_chunks(&chunk_ids)
            .await?;

        Ok(graph_context)
    }

    /// 生成最终答案：融合向量文本上下文与图谱上下文。
    pub async fn generate_answer(
        &self,
        question: &str,
        text_context: &str,
        graph_context: &str,
    ) -> Result<String, EngineError> {
        generate_answer_with_context(question, text_context, graph_context).await
    }

    /// 返回当前 Vault 的核心规模统计。
    pub async fn get_vault_stats(&self) -> Result<VaultStats, EngineError> {
        let document_count = self.state.vector_store.count_documents().await?;
        let chunk_count = self.state.vector_store.count_chunks().await?;
        let graph_node_count = self.state.vector_store.count_nodes().await?;

        Ok(VaultStats {
            document_count,
            chunk_count,
            graph_node_count,
        })
    }

    /// 启动异步守护任务，持续消费文件事件并触发解析、向量化与图谱提取流程。
    pub fn start_daemon(&mut self) -> Result<(), EngineError> {
        if self.daemon_task.is_some() {
            return Err(EngineError::DaemonAlreadyStarted);
        }

        let mut event_rx = self
            .event_rx
            .take()
            .ok_or(EngineError::EventChannelUnavailable)?;
        let state = Arc::clone(&self.state);

        let task = tokio::spawn(async move {
            info!("memori-core daemon started");

            match state.vector_store.load_from_db().await {
                Ok(loaded) => {
                    info!(
                        loaded = loaded,
                        "已成功从本地数据库加载 [{}] 条历史向量记忆", loaded
                    );
                }
                Err(err) => {
                    error!(
                        error = %err,
                        "加载本地数据库历史记忆失败，将以空缓存继续运行"
                    );
                }
            }

            while let Some(event) = event_rx.recv().await {
                match event.kind {
                    WatchEventKind::Created | WatchEventKind::Modified => {
                        process_file_event(&state, &event).await;
                    }
                    _ => {
                        debug!(
                            kind = ?event.kind,
                            path = %event.path.display(),
                            "忽略非 Created/Modified 事件"
                        );
                    }
                }
            }

            info!("memori-core event channel closed, daemon exiting");
            Ok(())
        });

        self.daemon_task = Some(task);
        Ok(())
    }

    /// 关闭引擎：
    /// 1) 优先停止 memori-vault（关闭发送端）；
    /// 2) 等待 daemon 消费完剩余事件后退出。
    pub async fn shutdown(mut self) -> Result<(), EngineError> {
        if let Some(memori_vault_handle) = self.memori_vault_handle.take() {
            memori_vault_handle.join().await?;
        }

        if let Some(daemon_task) = self.daemon_task.take() {
            daemon_task.await??;
        }

        Ok(())
    }
}

/// 提供给外部壳层（如 Tauri IPC）的答案合成入口。
pub async fn generate_answer_with_context(
    question: &str,
    text_context: &str,
    graph_context: &str,
) -> Result<String, EngineError> {
    generate_llm_answer(question, text_context, graph_context).await
}

async fn process_file_event(state: &Arc<AppState>, event: &WatchEvent) {
    let raw_text = match tokio::fs::read_to_string(&event.path).await {
        Ok(text) => text,
        Err(err) => {
            warn!(
                path = %event.path.display(),
                error = %err,
                "文件读取失败（可能被占用），已跳过"
            );
            return;
        }
    };

    let chunks = match parse_and_chunk(&event.path, &raw_text) {
        Ok(chunks) => {
            info!(
                path = %event.path.display(),
                chunk_count = chunks.len(),
                "文件 [{}] 已成功解析，共生成 [{}] 个文本块。",
                event.path.display(),
                chunks.len()
            );
            chunks
        }
        Err(err) => {
            warn!(
                path = %event.path.display(),
                error = %err,
                "解析失败，已跳过本次事件"
            );
            return;
        }
    };

    if chunks.is_empty() {
        debug!(path = %event.path.display(), "解析结果为空，跳过向量化与存储");
        return;
    }

    let mut embeddings = Vec::with_capacity(chunks.len());
    let mut graph_batch = Vec::with_capacity(chunks.len());

    for chunk in &chunks {
        // 在同一个 chunk 维度内并发执行 embedding 和图谱抽取。
        let (embedding_res, graph_res) = tokio::join!(
            state.ollama_client.embed_text(&chunk.content),
            extract_entities(&chunk.content)
        );

        match embedding_res {
            Ok(embedding) => embeddings.push(embedding),
            Err(err) => {
                error!(
                    path = %event.path.display(),
                    error = %err,
                    "无法连接本地大模型，请确保 Ollama 已启动"
                );
                return;
            }
        }

        match graph_res {
            Ok(graph_data) => graph_batch.push(graph_data),
            Err(err) => {
                warn!(
                    path = %chunk.file_path.display(),
                    chunk_index = chunk.chunk_index,
                    error = %err,
                    "图谱抽取失败，本 Chunk 跳过关系写入"
                );
                graph_batch.push(GraphData::default());
            }
        }
    }

    if let Err(err) = state
        .vector_store
        .insert_chunks(chunks.clone(), embeddings)
        .await
    {
        error!(
            path = %event.path.display(),
            error = %err,
            "向量落盘失败，本次事件已跳过但守护进程继续运行"
        );
        return;
    }

    for (chunk, graph_data) in chunks.iter().zip(graph_batch.into_iter()) {
        let chunk_id = match state
            .vector_store
            .resolve_chunk_id(&chunk.file_path, chunk.chunk_index)
            .await
        {
            Ok(Some(id)) => id,
            Ok(None) => {
                warn!(
                    path = %chunk.file_path.display(),
                    chunk_index = chunk.chunk_index,
                    "已写入向量，但未找到对应 chunk_id，跳过图谱落盘"
                );
                continue;
            }
            Err(err) => {
                error!(
                    path = %chunk.file_path.display(),
                    chunk_index = chunk.chunk_index,
                    error = %err,
                    "查询 chunk_id 失败，跳过图谱落盘"
                );
                continue;
            }
        };

        let node_count = graph_data.nodes.len();
        let edge_count = graph_data.edges.len();

        if let Err(err) = state
            .vector_store
            .insert_graph(chunk_id, graph_data.nodes, graph_data.edges)
            .await
        {
            error!(
                path = %chunk.file_path.display(),
                chunk_index = chunk.chunk_index,
                error = %err,
                "图谱落盘失败，守护进程继续运行"
            );
            continue;
        }

        info!(
            chunk_index = chunk.chunk_index,
            node_count = node_count,
            edge_count = edge_count,
            "从 Chunk [{}] 中成功提取 [{}] 个节点和 [{}] 条关系",
            chunk.chunk_index,
            node_count,
            edge_count
        );
    }
}
