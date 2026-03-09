mod graph_extractor;
mod llm_generator;

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::time::SystemTime;

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

pub const DEFAULT_MODEL_PROVIDER: &str = "ollama_local";
pub const DEFAULT_MODEL_ENDPOINT_OLLAMA: &str = "http://localhost:11434";
pub const DEFAULT_MODEL_ENDPOINT_OPENAI: &str = "https://api.openai.com";
pub const DEFAULT_OLLAMA_EMBED_MODEL: &str = "nomic-embed-text:latest";
pub const DEFAULT_CHAT_MODEL: &str = "qwen2.5:7b";
pub const DEFAULT_GRAPH_MODEL: &str = "qwen2.5:7b";
const DEFAULT_DB_FILE_NAME: &str = ".memori.db";
const MEMORI_DB_PATH_ENV: &str = "MEMORI_DB_PATH";
pub const MEMORI_MODEL_PROVIDER_ENV: &str = "MEMORI_MODEL_PROVIDER";
pub const MEMORI_MODEL_ENDPOINT_ENV: &str = "MEMORI_MODEL_ENDPOINT";
pub const MEMORI_MODEL_API_KEY_ENV: &str = "MEMORI_MODEL_API_KEY";
pub const MEMORI_CHAT_MODEL_ENV: &str = "MEMORI_CHAT_MODEL";
pub const MEMORI_GRAPH_MODEL_ENV: &str = "MEMORI_GRAPH_MODEL";
pub const MEMORI_EMBED_MODEL_ENV: &str = "MEMORI_EMBED_MODEL";

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
    pub embedding_client: OllamaEmbeddingClient,
}

impl AppState {
    pub fn new(db_path: impl Into<PathBuf>) -> Result<Self, StorageError> {
        let vector_store = Arc::new(SqliteStore::new(db_path.into())?);
        Ok(Self {
            parser: ParserStub,
            vector_store,
            embedding_client: OllamaEmbeddingClient::default(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelProvider {
    OllamaLocal,
    OpenAiCompatible,
}

impl ModelProvider {
    pub fn from_value(text: &str) -> Self {
        text.parse().unwrap_or(Self::OllamaLocal)
    }
}

impl FromStr for ModelProvider {
    type Err = &'static str;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text.trim().to_ascii_lowercase().as_str() {
            "ollama_local" => Ok(Self::OllamaLocal),
            "openai_compatible" => Ok(Self::OpenAiCompatible),
            _ => Err("unknown model provider"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeModelConfig {
    pub provider: ModelProvider,
    pub endpoint: String,
    pub api_key: Option<String>,
    pub chat_model: String,
    pub graph_model: String,
    pub embed_model: String,
}

pub fn resolve_runtime_model_config_from_env() -> RuntimeModelConfig {
    let provider = std::env::var(MEMORI_MODEL_PROVIDER_ENV)
        .map(|v| ModelProvider::from_value(&v))
        .unwrap_or(ModelProvider::OllamaLocal);

    let endpoint_default = match provider {
        ModelProvider::OllamaLocal => DEFAULT_MODEL_ENDPOINT_OLLAMA,
        ModelProvider::OpenAiCompatible => DEFAULT_MODEL_ENDPOINT_OPENAI,
    };
    let endpoint =
        std::env::var(MEMORI_MODEL_ENDPOINT_ENV).unwrap_or_else(|_| endpoint_default.to_string());

    let api_key = std::env::var(MEMORI_MODEL_API_KEY_ENV).ok().and_then(|v| {
        let trimmed = v.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });

    let chat_model =
        std::env::var(MEMORI_CHAT_MODEL_ENV).unwrap_or_else(|_| DEFAULT_CHAT_MODEL.to_string());
    let graph_model =
        std::env::var(MEMORI_GRAPH_MODEL_ENV).unwrap_or_else(|_| DEFAULT_GRAPH_MODEL.to_string());
    let embed_model = std::env::var(MEMORI_EMBED_MODEL_ENV)
        .unwrap_or_else(|_| DEFAULT_OLLAMA_EMBED_MODEL.to_string());

    RuntimeModelConfig {
        provider,
        endpoint,
        api_key,
        chat_model,
        graph_model,
        embed_model,
    }
}

/// 极简 Ollama Embedding 客户端。
#[derive(Debug, Clone)]
pub struct OllamaEmbeddingClient {
    http: reqwest::Client,
    provider: ModelProvider,
    base_url: String,
    api_key: Option<String>,
    model: String,
}

impl Default for OllamaEmbeddingClient {
    fn default() -> Self {
        let runtime = resolve_runtime_model_config_from_env();
        Self {
            http: reqwest::Client::new(),
            provider: runtime.provider,
            base_url: runtime.endpoint,
            api_key: runtime.api_key,
            model: runtime.embed_model,
        }
    }
}

impl OllamaEmbeddingClient {
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            provider: ModelProvider::OllamaLocal,
            base_url: base_url.into(),
            api_key: None,
            model: model.into(),
        }
    }

    pub async fn embed_text(&self, prompt: &str) -> Result<Vec<f32>, OllamaClientError> {
        if self.provider == ModelProvider::OpenAiCompatible {
            return self.embed_text_openai_compatible(&self.model, prompt).await;
        }

        match self.embed_text_with_model(&self.model, prompt).await {
            // Ollama 常见 tag 省略场景：`nomic-embed-text` 实际只有 `nomic-embed-text:latest`
            // 若命中 404 not found 且当前 model 无 tag，则自动回退一次。
            Err(OllamaClientError::HttpStatus { status, body })
                if status == 404 && body.contains("not found") && !self.model.contains(':') =>
            {
                let fallback_model = format!("{}:latest", self.model);
                self.embed_text_with_model(&fallback_model, prompt).await
            }
            other => other,
        }
    }

    async fn embed_text_with_model(
        &self,
        model: &str,
        prompt: &str,
    ) -> Result<Vec<f32>, OllamaClientError> {
        let url = format!("{}/api/embeddings", self.base_url.trim_end_matches('/'));

        let response = self
            .http
            .post(url)
            .json(&OllamaEmbeddingRequest { model, prompt })
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

    async fn embed_text_openai_compatible(
        &self,
        model: &str,
        prompt: &str,
    ) -> Result<Vec<f32>, OllamaClientError> {
        let url = format!("{}/v1/embeddings", self.base_url.trim_end_matches('/'));
        let mut request = self.http.post(url).json(&OpenAiEmbeddingRequest {
            model,
            input: prompt,
        });
        if let Some(key) = self.api_key.as_ref() {
            request = request.bearer_auth(key);
        }

        let response = request.send().await.map_err(OllamaClientError::Request)?;
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

        let parsed: OpenAiEmbeddingResponse =
            response.json().await.map_err(OllamaClientError::Request)?;

        let embedding = parsed
            .data
            .into_iter()
            .next()
            .map(|item| item.embedding)
            .unwrap_or_default();
        if embedding.is_empty() {
            return Err(OllamaClientError::EmptyEmbedding);
        }
        Ok(embedding)
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

#[derive(Debug, serde::Serialize)]
struct OpenAiEmbeddingRequest<'a> {
    model: &'a str,
    input: &'a str,
}

#[derive(Debug, serde::Deserialize)]
struct OpenAiEmbeddingResponse {
    data: Vec<OpenAiEmbeddingItem>,
}

#[derive(Debug, serde::Deserialize)]
struct OpenAiEmbeddingItem {
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
    watch_root: Option<PathBuf>,
}

impl MemoriEngine {
    /// 用现成 receiver 构造引擎（便于测试和外部注入）。
    pub fn new(state: Arc<AppState>, event_rx: mpsc::Receiver<WatchEvent>) -> Self {
        Self {
            state,
            event_rx: Some(event_rx),
            daemon_task: None,
            memori_vault_handle: None,
            watch_root: None,
        }
    }

    /// 快速引导：创建事件通道 + 启动 memori-vault 监听端 + 初始化 SQLite 存储。
    pub fn bootstrap(root: impl Into<PathBuf>) -> Result<Self, EngineError> {
        let config = MemoriVaultConfig::new(root);
        Self::bootstrap_with_config(config)
    }

    /// 通过配置引导引擎。
    pub fn bootstrap_with_config(config: MemoriVaultConfig) -> Result<Self, EngineError> {
        let watch_root = config.root.clone();
        let (event_tx, event_rx) = create_event_channel();
        let memori_vault_handle = spawn_memori_vault(config, event_tx)?;
        let db_path = resolve_db_path()?;

        let state = Arc::new(AppState::new(db_path)?);
        let mut engine = Self::new(state, event_rx);
        engine.memori_vault_handle = Some(memori_vault_handle);
        engine.watch_root = Some(watch_root);
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
        scope_paths: Option<&[PathBuf]>,
    ) -> Result<Vec<(DocumentChunk, f32)>, EngineError> {
        if query.trim().is_empty() || top_k == 0 {
            return Ok(Vec::new());
        }

        let query_embedding = self.state.embedding_client.embed_text(query).await?;
        let results = self
            .state
            .vector_store
            .search_similar_scoped(query_embedding, top_k, scope_paths.unwrap_or(&[]))
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
        let watch_root = self.watch_root.clone();

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

            if let Some(root) = watch_root {
                let existing_files = collect_supported_text_files_recursively(root.clone()).await;
                info!(
                    root = %root.display(),
                    file_count = existing_files.len(),
                    "启动时递归扫描完成，准备回灌子目录中的历史文档"
                );

                for path in existing_files {
                    let event = WatchEvent {
                        kind: WatchEventKind::Modified,
                        path,
                        old_path: None,
                        observed_at: SystemTime::now(),
                    };
                    process_file_event(&state, &event).await;
                }
            }

            while let Some(event) = event_rx.recv().await {
                match event.kind {
                    WatchEventKind::Created
                    | WatchEventKind::Modified
                    | WatchEventKind::Renamed => {
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

fn resolve_db_path() -> Result<PathBuf, EngineError> {
    if let Ok(path) = std::env::var(MEMORI_DB_PATH_ENV) {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }

    Ok(std::env::current_dir()
        .map_err(EngineError::CurrentDir)?
        .join(DEFAULT_DB_FILE_NAME))
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

    // 优先完成 embedding 与向量落盘，避免图谱抽取耗时导致 stats 长时间保持 0。
    for chunk in &chunks {
        match state.embedding_client.embed_text(&chunk.content).await {
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

    for chunk in &chunks {
        let graph_data = match extract_entities(&chunk.content).await {
            Ok(graph_data) => graph_data,
            Err(err) => {
                warn!(
                    path = %chunk.file_path.display(),
                    chunk_index = chunk.chunk_index,
                    error = %err,
                    "图谱抽取失败，本 Chunk 跳过关系写入"
                );
                GraphData::default()
            }
        };

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

async fn collect_supported_text_files_recursively(root: PathBuf) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![root];

    while let Some(dir) = stack.pop() {
        let mut read_dir = match tokio::fs::read_dir(&dir).await {
            Ok(reader) => reader,
            Err(err) => {
                warn!(
                    path = %dir.display(),
                    error = %err,
                    "递归扫描目录失败，已跳过该目录"
                );
                continue;
            }
        };

        loop {
            let next = match read_dir.next_entry().await {
                Ok(entry) => entry,
                Err(err) => {
                    warn!(
                        path = %dir.display(),
                        error = %err,
                        "读取目录项失败，已跳过剩余目录项"
                    );
                    break;
                }
            };

            let Some(entry) = next else {
                break;
            };

            let path = entry.path();
            match entry.file_type().await {
                Ok(file_type) if file_type.is_dir() => {
                    stack.push(path);
                }
                Ok(file_type) if file_type.is_file() => {
                    if is_supported_text_file(&path) {
                        files.push(path);
                    }
                }
                Ok(_) => {}
                Err(err) => {
                    warn!(
                        path = %path.display(),
                        error = %err,
                        "读取文件类型失败，已跳过该路径"
                    );
                }
            }
        }
    }

    files.sort();
    files
}

fn is_supported_text_file(path: &std::path::Path) -> bool {
    let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
        return false;
    };
    ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("txt")
}
