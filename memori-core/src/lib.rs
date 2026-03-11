mod graph_extractor;
mod llm_generator;

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Instant;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{collections::HashMap, hash::Hash, hash::Hasher};

use graph_extractor::extract_entities;
use llm_generator::generate_answer as generate_llm_answer;
pub use memori_parser::DocumentChunk;
use memori_parser::{ParserStub, parse_and_chunk};
use memori_storage::{RebuildState, SqliteStore, StorageError};
use memori_vault::{
    MemoriVaultConfig, MemoriVaultError, MemoriVaultHandle, WatchEvent, WatchEventKind,
    create_event_channel, spawn_memori_vault,
};
use thiserror::Error;
use tokio::sync::{RwLock, mpsc};
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

pub const DEFAULT_MODEL_PROVIDER: &str = "ollama_local";
pub const DEFAULT_MODEL_ENDPOINT_OLLAMA: &str = "http://localhost:11434";
pub const DEFAULT_MODEL_ENDPOINT_OPENAI: &str = "https://api.openai.com";
pub const DEFAULT_OLLAMA_EMBED_MODEL: &str = "nomic-embed-text:latest";
pub const DEFAULT_CHAT_MODEL: &str = "qwen2.5:7b";
pub const DEFAULT_GRAPH_MODEL: &str = "qwen2.5:7b";
const DEFAULT_DB_FILE_NAME: &str = ".memori.db";
pub const MEMORI_DB_PATH_ENV: &str = "MEMORI_DB_PATH";
pub const MEMORI_MODEL_PROVIDER_ENV: &str = "MEMORI_MODEL_PROVIDER";
pub const MEMORI_MODEL_ENDPOINT_ENV: &str = "MEMORI_MODEL_ENDPOINT";
pub const MEMORI_MODEL_API_KEY_ENV: &str = "MEMORI_MODEL_API_KEY";
pub const MEMORI_CHAT_MODEL_ENV: &str = "MEMORI_CHAT_MODEL";
pub const MEMORI_GRAPH_MODEL_ENV: &str = "MEMORI_GRAPH_MODEL";
pub const MEMORI_EMBED_MODEL_ENV: &str = "MEMORI_EMBED_MODEL";
const QUERY_EMBEDDING_CACHE_SIZE: usize = 256;
const QUERY_EMBEDDING_CACHE_TTL_SECS: i64 = 300;
const DEFAULT_DOC_TOP_K: usize = 12;
const DEFAULT_CHUNK_CANDIDATE_K: usize = 20;
const DEFAULT_FINAL_ANSWER_K: usize = 6;
const RRF_K: f64 = 60.0;

fn is_supported_index_file(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("txt"))
        .unwrap_or(false)
}

fn is_likely_directory_path(path: &std::path::Path) -> bool {
    path.extension().is_none()
}

async fn set_runtime_idle(state: &Arc<AppState>, last_error: Option<String>) {
    let mut runtime = state.indexing_runtime.write().await;
    runtime.phase = "idle".to_string();
    runtime.last_error = last_error;
}

async fn ensure_search_ready(state: &Arc<AppState>) -> Result<(), EngineError> {
    let metadata = state.vector_store.read_index_metadata().await?;
    match metadata.rebuild_state {
        RebuildState::Ready => Ok(()),
        RebuildState::Required => Err(EngineError::IndexUnavailable {
            reason: metadata.rebuild_reason,
        }),
        RebuildState::Rebuilding => Err(EngineError::IndexRebuildInProgress {
            reason: metadata.rebuild_reason,
        }),
    }
}

/// 前端/CLI 可消费的 Vault 统计信息。
#[derive(Debug, Clone, serde::Serialize)]
pub struct VaultStats {
    pub document_count: u64,
    pub chunk_count: u64,
    pub graph_node_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum IndexingMode {
    #[default]
    Continuous,
    Manual,
    Scheduled,
}

impl IndexingMode {
    pub fn from_value(text: &str) -> Self {
        match text.trim().to_ascii_lowercase().as_str() {
            "manual" => Self::Manual,
            "scheduled" => Self::Scheduled,
            _ => Self::Continuous,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Continuous => "continuous",
            Self::Manual => "manual",
            Self::Scheduled => "scheduled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ResourceBudget {
    #[default]
    Low,
    Balanced,
    Fast,
}

impl ResourceBudget {
    pub fn from_value(text: &str) -> Self {
        match text.trim().to_ascii_lowercase().as_str() {
            "balanced" => Self::Balanced,
            "fast" => Self::Fast,
            _ => Self::Low,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Balanced => "balanced",
            Self::Fast => "fast",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScheduleWindow {
    pub start: String,
    pub end: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct IndexingConfig {
    pub mode: IndexingMode,
    pub resource_budget: ResourceBudget,
    pub schedule_window: Option<ScheduleWindow>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct IndexingStatus {
    pub phase: String,
    pub indexed_docs: u64,
    pub indexed_chunks: u64,
    pub graphed_chunks: u64,
    pub graph_backlog: u64,
    pub last_scan_at: Option<i64>,
    pub last_error: Option<String>,
    pub paused: bool,
    pub mode: IndexingMode,
    pub resource_budget: ResourceBudget,
    pub rebuild_state: String,
    pub rebuild_reason: Option<String>,
    pub index_format_version: u32,
    pub parser_format_version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AskStatus {
    Answered,
    InsufficientEvidence,
    ModelFailedWithEvidence,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CitationItem {
    pub index: usize,
    pub file_path: String,
    pub relative_path: String,
    pub chunk_index: usize,
    pub heading_path: Vec<String>,
    pub excerpt: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EvidenceItem {
    pub file_path: String,
    pub relative_path: String,
    pub chunk_index: usize,
    pub heading_path: Vec<String>,
    pub block_kind: String,
    pub document_reason: String,
    pub reason: String,
    pub document_rank: usize,
    pub chunk_rank: usize,
    pub document_raw_score: Option<f64>,
    pub lexical_raw_score: Option<f64>,
    pub dense_raw_score: Option<f32>,
    pub final_score: f64,
    pub content: String,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct RetrievalMetrics {
    pub query_analysis_ms: u64,
    pub doc_recall_ms: u64,
    pub doc_exact_ms: u64,
    pub doc_strict_lexical_ms: u64,
    pub doc_lexical_ms: u64,
    pub doc_merge_ms: u64,
    pub chunk_strict_lexical_ms: u64,
    pub chunk_lexical_ms: u64,
    pub chunk_dense_ms: u64,
    pub merge_ms: u64,
    pub answer_ms: u64,
    pub doc_candidate_count: usize,
    pub chunk_candidate_count: usize,
    pub final_evidence_count: usize,
    pub query_flags: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AskResponseStructured {
    pub status: AskStatus,
    pub answer: String,
    pub question: String,
    pub scope_paths: Vec<String>,
    pub citations: Vec<CitationItem>,
    pub evidence: Vec<EvidenceItem>,
    pub metrics: RetrievalMetrics,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RetrievalInspection {
    pub status: AskStatus,
    pub question: String,
    pub scope_paths: Vec<String>,
    pub citations: Vec<CitationItem>,
    pub evidence: Vec<EvidenceItem>,
    pub metrics: RetrievalMetrics,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RuntimeRetrievalBaseline {
    pub watch_root: Option<String>,
    pub resolved_db_path: String,
    pub embedding_model_key: String,
    pub embedding_dim: usize,
    pub indexed_document_count: u64,
    pub indexed_chunk_count: u64,
    pub rebuild_state: String,
}

#[derive(Debug, Clone)]
struct QueryPreparation {
    analysis: QueryAnalysis,
    metrics: RetrievalMetrics,
}

#[derive(Debug, Clone)]
struct DocumentCandidate {
    file_path: String,
    relative_path: String,
    file_name: String,
    is_code_document: bool,
    document_reason: String,
    document_rank: usize,
    document_raw_score: Option<f64>,
    exact_signal_score: Option<i64>,
    exact_path_score: Option<i64>,
    exact_symbol_score: Option<i64>,
    document_filename_score: Option<i64>,
    document_final_score: f64,
    has_exact_signal: bool,
    has_exact_path_signal: bool,
    has_exact_symbol_signal: bool,
    has_filename_signal: bool,
    has_strict_lexical: bool,
    has_broad_lexical: bool,
}

#[derive(Debug, Clone)]
struct MergedEvidence {
    chunk: DocumentChunk,
    relative_path: String,
    document_reason: String,
    document_rank: usize,
    document_raw_score: Option<f64>,
    document_has_exact_signal: bool,
    document_has_filename_signal: bool,
    document_has_strict_lexical: bool,
    lexical_strict_rank: Option<usize>,
    lexical_broad_rank: Option<usize>,
    lexical_raw_score: Option<f64>,
    dense_rank: Option<usize>,
    dense_raw_score: Option<f32>,
    final_score: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QueryIntent {
    RepoLookup,
    RepoQuestion,
    ExternalFact,
    SecretRequest,
    MissingFileLookup,
}

impl QueryIntent {
    fn as_str(self) -> &'static str {
        match self {
            Self::RepoLookup => "repo_lookup",
            Self::RepoQuestion => "repo_question",
            Self::ExternalFact => "external_fact",
            Self::SecretRequest => "secret_request",
            Self::MissingFileLookup => "missing_file_lookup",
        }
    }
}

#[derive(Debug, Clone)]
struct QueryAnalysis {
    normalized_query: String,
    lexical_query: String,
    document_routing_terms: Vec<String>,
    chunk_terms: Vec<String>,
    filename_like_terms: Vec<String>,
    identifier_terms: Vec<String>,
    query_intent: QueryIntent,
    flags: QueryFlags,
}

#[derive(Debug, Clone, Default)]
struct QueryFlags {
    has_cjk: bool,
    has_ascii_identifier: bool,
    has_path_like_token: bool,
    is_lookup_like: bool,
    token_count: usize,
}

#[derive(Debug, Clone)]
struct EmbeddingCacheItem {
    embedding: Vec<f32>,
    cached_at: i64,
}

#[derive(Debug, Default, Clone)]
struct IndexingRuntimeState {
    phase: String,
    last_scan_at: Option<i64>,
    last_error: Option<String>,
    paused: bool,
    config: IndexingConfig,
}

/// 全局共享状态。
/// 当前持有 parser 占位、SQLite 持久化存储与本地 Ollama 客户端。
#[derive(Debug)]
pub struct AppState {
    pub parser: ParserStub,
    pub vector_store: Arc<SqliteStore>,
    pub embedding_client: OllamaEmbeddingClient,
    db_path: PathBuf,
    query_embedding_cache: Arc<RwLock<HashMap<String, EmbeddingCacheItem>>>,
    indexing_runtime: Arc<RwLock<IndexingRuntimeState>>,
}

impl AppState {
    pub fn new(db_path: impl Into<PathBuf>) -> Result<Self, StorageError> {
        let db_path = db_path.into();
        let vector_store = Arc::new(SqliteStore::new(&db_path)?);
        Ok(Self {
            parser: ParserStub,
            vector_store,
            embedding_client: OllamaEmbeddingClient::default(),
            db_path,
            query_embedding_cache: Arc::new(RwLock::new(HashMap::new())),
            indexing_runtime: Arc::new(RwLock::new(IndexingRuntimeState {
                phase: "idle".to_string(),
                last_scan_at: None,
                last_error: None,
                paused: false,
                config: IndexingConfig::default(),
            })),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EgressMode {
    #[default]
    LocalOnly,
    Allowlist,
}

impl EgressMode {
    pub fn from_value(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "allowlist" => Self::Allowlist,
            _ => Self::LocalOnly,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct EnterpriseModelPolicy {
    pub egress_mode: EgressMode,
    pub allowed_model_endpoints: Vec<String>,
    pub allowed_models: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PolicyViolation {
    pub code: String,
    pub message: String,
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

pub fn normalize_policy_endpoint(endpoint: &str) -> String {
    let trimmed = endpoint.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if let Ok(url) = reqwest::Url::parse(trimmed) {
        let host = url.host_str().map(|value| value.to_ascii_lowercase());
        let mut normalized = format!("{}://", url.scheme().to_ascii_lowercase());
        if let Some(host) = host {
            normalized.push_str(&host);
        } else {
            return trimmed.trim_end_matches('/').to_ascii_lowercase();
        }
        if let Some(port) = url.port() {
            normalized.push(':');
            normalized.push_str(&port.to_string());
        }
        let path = url.path().trim_end_matches('/');
        if !path.is_empty() && path != "/" {
            normalized.push_str(path);
        }
        return normalized;
    }

    trimmed.trim_end_matches('/').to_ascii_lowercase()
}

pub fn validate_provider_request(
    policy: &EnterpriseModelPolicy,
    provider: ModelProvider,
    endpoint: &str,
    models: &[String],
) -> Result<(), PolicyViolation> {
    if provider == ModelProvider::OllamaLocal {
        return Ok(());
    }

    if policy.egress_mode == EgressMode::LocalOnly {
        return Err(PolicyViolation {
            code: "policy_violation".to_string(),
            message: "Remote model endpoint blocked by enterprise policy".to_string(),
        });
    }

    let normalized_endpoint = normalize_policy_endpoint(endpoint);
    let normalized_allowlist = policy
        .allowed_model_endpoints
        .iter()
        .map(|item| normalize_policy_endpoint(item))
        .filter(|item| !item.is_empty())
        .collect::<std::collections::HashSet<_>>();
    if !normalized_allowlist.is_empty() && !normalized_allowlist.contains(&normalized_endpoint) {
        return Err(PolicyViolation {
            code: "remote_endpoint_not_allowlisted".to_string(),
            message: format!(
                "Remote model endpoint blocked by enterprise policy: {}",
                endpoint.trim()
            ),
        });
    }

    let normalized_models = policy
        .allowed_models
        .iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect::<std::collections::HashSet<_>>();
    if !normalized_models.is_empty() {
        for model in models {
            let trimmed = model.trim();
            if trimmed.is_empty() {
                continue;
            }
            if !normalized_models.contains(trimmed) {
                return Err(PolicyViolation {
                    code: "model_not_allowlisted".to_string(),
                    message: format!("Remote model blocked by enterprise policy: {trimmed}"),
                });
            }
        }
    }

    Ok(())
}

pub fn validate_runtime_model_settings(
    policy: &EnterpriseModelPolicy,
    runtime: &RuntimeModelConfig,
) -> Result<(), PolicyViolation> {
    validate_provider_request(
        policy,
        runtime.provider,
        &runtime.endpoint,
        &[
            runtime.chat_model.clone(),
            runtime.graph_model.clone(),
            runtime.embed_model.clone(),
        ],
    )
    .map_err(|violation| match violation.code.as_str() {
        "policy_violation" => PolicyViolation {
            code: "runtime_blocked_by_policy".to_string(),
            message: "Runtime model configuration rejected before startup".to_string(),
        },
        _ => violation,
    })
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

    pub fn model_name(&self) -> &str {
        &self.model
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

    #[error("索引不可用：检测到旧索引已失效，需完成全量重建后才能继续检索")]
    IndexUnavailable { reason: Option<String> },

    #[error("索引升级中：全量重建完成前暂不可检索")]
    IndexRebuildInProgress { reason: Option<String> },

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
    graph_worker_task: Option<JoinHandle<Result<(), EngineError>>>,
    memori_vault_handle: Option<MemoriVaultHandle>,
    watch_root: Option<PathBuf>,
    graph_notify_tx: Option<mpsc::Sender<()>>,
}

impl MemoriEngine {
    /// 用现成 receiver 构造引擎（便于测试和外部注入）。
    pub fn new(state: Arc<AppState>, event_rx: mpsc::Receiver<WatchEvent>) -> Self {
        Self {
            state,
            event_rx: Some(event_rx),
            daemon_task: None,
            graph_worker_task: None,
            memori_vault_handle: None,
            watch_root: None,
            graph_notify_tx: None,
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

        ensure_search_ready(&self.state).await?;
        let query_embedding = self.embed_query_cached(query).await?;
        let results = self
            .state
            .vector_store
            .search_similar_scoped(query_embedding, top_k, scope_paths.unwrap_or(&[]))
            .await?;

        Ok(results)
    }

    pub async fn ask_structured(
        &self,
        query: &str,
        lang: Option<&str>,
        scope_paths: Option<&[PathBuf]>,
        final_answer_k: Option<usize>,
    ) -> Result<AskResponseStructured, EngineError> {
        let final_answer_k = final_answer_k
            .filter(|value| (1..=50).contains(value))
            .unwrap_or(DEFAULT_FINAL_ANSWER_K);
        let mut inspection = self
            .retrieve_structured(query, scope_paths, Some(final_answer_k))
            .await?;
        if inspection.status != AskStatus::Answered {
            return Ok(AskResponseStructured {
                status: inspection.status,
                answer: String::new(),
                question: inspection.question,
                scope_paths: inspection.scope_paths,
                citations: inspection.citations,
                evidence: inspection.evidence,
                metrics: inspection.metrics,
            });
        }

        let final_evidence = build_merged_evidence_from_items(&inspection.evidence);
        let answer_question = build_answer_question(&inspection.question, lang);
        let text_context = build_text_context_from_evidence(&final_evidence);
        let graph_seed = final_evidence
            .iter()
            .map(|item| (item.chunk.clone(), item.final_score as f32))
            .collect::<Vec<_>>();
        let graph_context = match self.get_graph_context_for_results(&graph_seed).await {
            Ok(context) => context,
            Err(err) => {
                warn!(error = %err, "图谱上下文构建失败，降级为纯文本上下文回答");
                String::new()
            }
        };

        let answer_started_at = Instant::now();
        let answer = match self
            .generate_answer(&answer_question, &text_context, &graph_context)
            .await
        {
            Ok(answer) => {
                inspection.metrics.answer_ms = elapsed_ms_u64(answer_started_at);
                answer
            }
            Err(err) => {
                warn!(error = %err, "答案生成失败，保留证据链返回");
                inspection.metrics.answer_ms = elapsed_ms_u64(answer_started_at);
                return Ok(AskResponseStructured {
                    status: AskStatus::ModelFailedWithEvidence,
                    answer: String::new(),
                    question: inspection.question,
                    scope_paths: inspection.scope_paths,
                    citations: inspection.citations,
                    evidence: inspection.evidence,
                    metrics: inspection.metrics,
                });
            }
        };

        Ok(AskResponseStructured {
            status: AskStatus::Answered,
            answer,
            question: inspection.question,
            scope_paths: inspection.scope_paths,
            citations: inspection.citations,
            evidence: inspection.evidence,
            metrics: inspection.metrics,
        })
    }

    pub async fn retrieve_structured(
        &self,
        query: &str,
        scope_paths: Option<&[PathBuf]>,
        final_answer_k: Option<usize>,
    ) -> Result<RetrievalInspection, EngineError> {
        if query.trim().is_empty() {
            return self
                .retrieve_structured_with_embedding(query, Vec::new(), scope_paths, final_answer_k)
                .await;
        }
        let query_embedding = self.embed_query_cached(query).await?;
        self.retrieve_structured_with_embedding(query, query_embedding, scope_paths, final_answer_k)
            .await
    }

    pub async fn retrieve_structured_with_embedding(
        &self,
        query: &str,
        query_embedding: Vec<f32>,
        scope_paths: Option<&[PathBuf]>,
        final_answer_k: Option<usize>,
    ) -> Result<RetrievalInspection, EngineError> {
        let question = query.trim().to_string();
        let final_answer_k = final_answer_k
            .filter(|value| (1..=50).contains(value))
            .unwrap_or(DEFAULT_FINAL_ANSWER_K);
        let normalized_scope_paths = scope_paths
            .unwrap_or(&[])
            .iter()
            .filter(|path| !path.as_os_str().is_empty())
            .cloned()
            .collect::<Vec<_>>();
        let serialized_scope_paths = normalized_scope_paths
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect::<Vec<_>>();

        if question.is_empty() {
            return Ok(RetrievalInspection {
                status: AskStatus::InsufficientEvidence,
                question,
                scope_paths: serialized_scope_paths,
                citations: Vec::new(),
                evidence: Vec::new(),
                metrics: RetrievalMetrics::default(),
            });
        }

        ensure_search_ready(&self.state).await?;
        let QueryPreparation {
            mut analysis,
            mut metrics,
        } = prepare_query_for_retrieval(&question);

        let doc_started_at = Instant::now();
        let candidate_docs = self
            .resolve_candidate_documents(&analysis, &normalized_scope_paths, &mut metrics)
            .await?;
        metrics.doc_recall_ms = elapsed_ms_u64(doc_started_at);
        metrics.doc_candidate_count = candidate_docs.len();

        if candidate_docs.is_empty() {
            if should_mark_missing_file_lookup_intent(&analysis) {
                analysis.query_intent = QueryIntent::MissingFileLookup;
                metrics
                    .query_flags
                    .retain(|flag| !flag.starts_with("intent:"));
                metrics
                    .query_flags
                    .push(format!("intent:{}", analysis.query_intent.as_str()));
            }
            return Ok(RetrievalInspection {
                status: AskStatus::InsufficientEvidence,
                question,
                scope_paths: serialized_scope_paths,
                citations: Vec::new(),
                evidence: Vec::new(),
                metrics,
            });
        }

        let candidate_scope_paths = candidate_docs
            .iter()
            .map(|doc| PathBuf::from(&doc.file_path))
            .collect::<Vec<_>>();
        let strict_lexical_started_at = Instant::now();
        let strict_lexical_matches = self
            .state
            .vector_store
            .search_chunks_fts_strict(
                &query_string_for_terms(&analysis.chunk_terms, &analysis.normalized_query),
                DEFAULT_CHUNK_CANDIDATE_K,
                &candidate_scope_paths,
            )
            .await?;
        metrics.chunk_strict_lexical_ms = elapsed_ms_u64(strict_lexical_started_at);

        let lexical_started_at = Instant::now();
        let lexical_matches = self
            .state
            .vector_store
            .search_chunks_fts(
                &query_string_for_terms(&analysis.chunk_terms, &analysis.normalized_query),
                DEFAULT_CHUNK_CANDIDATE_K,
                &candidate_scope_paths,
            )
            .await?;
        metrics.chunk_lexical_ms = elapsed_ms_u64(lexical_started_at);

        let dense_started_at = Instant::now();
        let dense_matches = self
            .state
            .vector_store
            .search_similar_scoped(
                query_embedding,
                DEFAULT_CHUNK_CANDIDATE_K,
                &candidate_scope_paths,
            )
            .await?;
        metrics.chunk_dense_ms = elapsed_ms_u64(dense_started_at);

        let merge_started_at = Instant::now();
        let merged = merge_chunk_evidence(
            &analysis,
            &candidate_docs,
            strict_lexical_matches,
            lexical_matches,
            dense_matches,
        );
        metrics.merge_ms = elapsed_ms_u64(merge_started_at);
        metrics.chunk_candidate_count = merged.len();

        if should_refuse_for_insufficient_evidence(&analysis, &merged) {
            return Ok(RetrievalInspection {
                status: AskStatus::InsufficientEvidence,
                question,
                scope_paths: serialized_scope_paths,
                citations: build_citations(&merged),
                evidence: build_evidence_items(&merged),
                metrics,
            });
        }

        let final_evidence = merged.into_iter().take(final_answer_k).collect::<Vec<_>>();
        metrics.final_evidence_count = final_evidence.len();
        let status = if final_evidence.len() < 2 {
            AskStatus::InsufficientEvidence
        } else {
            AskStatus::Answered
        };

        Ok(RetrievalInspection {
            status,
            question,
            scope_paths: serialized_scope_paths,
            citations: build_citations(&final_evidence),
            evidence: build_evidence_items(&final_evidence),
            metrics,
        })
    }

    async fn embed_query_cached(&self, query: &str) -> Result<Vec<f32>, EngineError> {
        let query_key = query.trim().to_string();
        let now = unix_now_secs();
        let cached = {
            let cache_guard = self.state.query_embedding_cache.read().await;
            cache_guard.get(&query_key).and_then(|item| {
                if now - item.cached_at <= QUERY_EMBEDDING_CACHE_TTL_SECS {
                    Some(item.embedding.clone())
                } else {
                    None
                }
            })
        };

        if let Some(embedding) = cached {
            return Ok(embedding);
        }

        let embedding = self.state.embedding_client.embed_text(query).await?;
        let mut cache_guard = self.state.query_embedding_cache.write().await;
        if cache_guard.len() >= QUERY_EMBEDDING_CACHE_SIZE {
            let stale_key = cache_guard
                .iter()
                .min_by_key(|(_, item)| item.cached_at)
                .map(|(key, _)| key.clone());
            if let Some(stale_key) = stale_key {
                cache_guard.remove(&stale_key);
            }
        }
        cache_guard.insert(
            query_key,
            EmbeddingCacheItem {
                embedding: embedding.clone(),
                cached_at: now,
            },
        );
        Ok(embedding)
    }

    async fn resolve_candidate_documents(
        &self,
        analysis: &QueryAnalysis,
        scope_paths: &[PathBuf],
        metrics: &mut RetrievalMetrics,
    ) -> Result<Vec<DocumentCandidate>, EngineError> {
        let mut by_path = HashMap::<String, DocumentCandidate>::new();
        let file_scopes = scope_paths
            .iter()
            .filter(|path| is_supported_index_file(path))
            .cloned()
            .collect::<Vec<_>>();

        if !file_scopes.is_empty() {
            for (index, file_path) in file_scopes.iter().enumerate() {
                if let Some(record) = self
                    .state
                    .vector_store
                    .get_document_by_file_path(file_path)
                    .await?
                {
                    let is_code_document = is_code_document_path(&record.relative_path);
                    by_path.insert(
                        record.file_path.clone(),
                        DocumentCandidate {
                            file_path: record.file_path,
                            relative_path: record.relative_path,
                            file_name: record.file_name,
                            is_code_document,
                            document_reason: "scope".to_string(),
                            document_rank: index + 1,
                            document_raw_score: None,
                            exact_signal_score: None,
                            exact_path_score: None,
                            exact_symbol_score: None,
                            document_filename_score: None,
                            document_final_score: 10_000.0 - index as f64,
                            has_exact_signal: false,
                            has_exact_path_signal: false,
                            has_exact_symbol_signal: false,
                            has_filename_signal: false,
                            has_strict_lexical: true,
                            has_broad_lexical: true,
                        },
                    );
                }
            }
        }

        let file_only_scope = !scope_paths.is_empty() && file_scopes.len() == scope_paths.len();
        if file_only_scope && !by_path.is_empty() {
            let mut docs = by_path.into_values().collect::<Vec<_>>();
            docs.sort_by(|a, b| a.document_rank.cmp(&b.document_rank));
            return Ok(docs);
        }

        let doc_exact_started_at = Instant::now();
        let exact_docs = self
            .state
            .vector_store
            .search_documents_signal(
                &document_signal_query(analysis),
                DEFAULT_DOC_TOP_K,
                scope_paths,
            )
            .await?;
        metrics.doc_exact_ms = elapsed_ms_u64(doc_exact_started_at);

        let doc_strict_started_at = Instant::now();
        let strict_docs = self
            .state
            .vector_store
            .search_documents_fts_strict(
                &query_string_for_terms(&analysis.document_routing_terms, &analysis.lexical_query),
                DEFAULT_DOC_TOP_K,
                scope_paths,
            )
            .await?;
        metrics.doc_strict_lexical_ms = elapsed_ms_u64(doc_strict_started_at);

        let doc_lexical_started_at = Instant::now();
        let routed_docs = self
            .state
            .vector_store
            .search_documents_fts(
                &query_string_for_terms(&analysis.document_routing_terms, &analysis.lexical_query),
                DEFAULT_DOC_TOP_K,
                scope_paths,
            )
            .await?;
        metrics.doc_lexical_ms = elapsed_ms_u64(doc_lexical_started_at);

        let merge_started_at = Instant::now();
        let merged_docs = merge_document_candidates(analysis, exact_docs, strict_docs, routed_docs);
        metrics.doc_merge_ms = elapsed_ms_u64(merge_started_at);

        for doc in merged_docs {
            by_path.entry(doc.file_path.clone()).or_insert(doc);
        }

        let mut docs = by_path.into_values().collect::<Vec<_>>();
        docs.sort_by(|a, b| {
            b.document_final_score
                .total_cmp(&a.document_final_score)
                .then_with(|| a.document_rank.cmp(&b.document_rank))
                .then_with(|| a.file_name.cmp(&b.file_name))
        });
        for (index, doc) in docs.iter_mut().enumerate() {
            doc.document_rank = index + 1;
        }
        Ok(docs)
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

    pub async fn get_indexing_status(&self) -> Result<IndexingStatus, EngineError> {
        let runtime = self.state.indexing_runtime.read().await;
        let metadata = self.state.vector_store.read_index_metadata().await?;
        let indexed_docs = self.state.vector_store.count_documents().await?;
        let indexed_chunks = self.state.vector_store.count_chunks().await?;
        let graphed_chunks = self.state.vector_store.count_graphed_chunks().await?;
        let graph_backlog = self.state.vector_store.count_graph_backlog().await?;

        Ok(IndexingStatus {
            phase: runtime.phase.clone(),
            indexed_docs,
            indexed_chunks,
            graphed_chunks,
            graph_backlog,
            last_scan_at: runtime.last_scan_at,
            last_error: runtime.last_error.clone(),
            paused: runtime.paused,
            mode: runtime.config.mode,
            resource_budget: runtime.config.resource_budget,
            rebuild_state: metadata.rebuild_state.as_str().to_string(),
            rebuild_reason: metadata.rebuild_reason,
            index_format_version: metadata.index_format_version,
            parser_format_version: metadata.parser_format_version,
        })
    }

    pub async fn get_runtime_retrieval_baseline(
        &self,
    ) -> Result<RuntimeRetrievalBaseline, EngineError> {
        let metadata = self.state.vector_store.read_index_metadata().await?;
        let indexed_document_count = self.state.vector_store.count_documents().await?;
        let indexed_chunk_count = self.state.vector_store.count_chunks().await?;
        let embedding_dim = self
            .state
            .vector_store
            .embedding_dimension()
            .await?
            .unwrap_or_default();

        Ok(RuntimeRetrievalBaseline {
            watch_root: self
                .watch_root
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
            resolved_db_path: self.state.db_path.to_string_lossy().to_string(),
            embedding_model_key: self.state.embedding_client.model_name().to_string(),
            embedding_dim,
            indexed_document_count,
            indexed_chunk_count,
            rebuild_state: metadata.rebuild_state.as_str().to_string(),
        })
    }

    pub async fn set_indexing_config(&self, config: IndexingConfig) {
        let mut runtime = self.state.indexing_runtime.write().await;
        runtime.config = config;
    }

    pub async fn pause_indexing(&self) {
        let mut runtime = self.state.indexing_runtime.write().await;
        runtime.paused = true;
    }

    pub async fn resume_indexing(&self) {
        let mut runtime = self.state.indexing_runtime.write().await;
        runtime.paused = false;
    }

    pub async fn trigger_reindex(&self) -> Result<(), EngineError> {
        let Some(root) = self.watch_root.clone() else {
            return Ok(());
        };
        run_full_rebuild(
            &self.state,
            &root,
            self.graph_notify_tx.as_ref(),
            "manual_reindex",
        )
        .await
    }

    pub async fn prepare_retrieval_index(&self) -> Result<(), EngineError> {
        let Some(root) = self.watch_root.clone() else {
            return Ok(());
        };

        let metadata = self.state.vector_store.read_index_metadata().await?;
        if metadata.rebuild_state == RebuildState::Required
            || metadata.rebuild_state == RebuildState::Rebuilding
        {
            return run_full_rebuild(
                &self.state,
                &root,
                None,
                metadata
                    .rebuild_reason
                    .as_deref()
                    .unwrap_or("index_upgrade_required"),
            )
            .await;
        }

        match self.state.vector_store.load_from_db().await {
            Ok(loaded) => {
                info!(
                    loaded = loaded,
                    "已从本地数据库加载检索回归所需的历史向量缓存"
                );
            }
            Err(err) => {
                warn!(error = %err, "回归前加载本地数据库缓存失败，将继续执行一次全量扫描");
            }
        }

        {
            let mut runtime = self.state.indexing_runtime.write().await;
            runtime.phase = "scanning".to_string();
            runtime.last_scan_at = Some(unix_now_secs());
            runtime.last_error = None;
        }

        let existing_files = collect_supported_text_files_recursively(root.clone()).await;
        for path in existing_files {
            let event = WatchEvent {
                kind: WatchEventKind::Modified,
                path,
                old_path: None,
                observed_at: SystemTime::now(),
            };
            process_file_event(&self.state, &event, None, Some(&root), false).await;
        }

        set_runtime_idle(&self.state, None).await;
        let mut runtime = self.state.indexing_runtime.write().await;
        runtime.last_scan_at = Some(unix_now_secs());
        Ok(())
    }

    /// 启动异步守护任务，持续消费文件事件并触发解析、向量化与图谱提取流程。
    pub fn start_daemon(&mut self) -> Result<(), EngineError> {
        if self.daemon_task.is_some() {
            return Err(EngineError::DaemonAlreadyStarted);
        }

        let (graph_notify_tx, graph_notify_rx) = mpsc::channel::<()>(32);
        self.graph_notify_tx = Some(graph_notify_tx.clone());

        let mut event_rx = self
            .event_rx
            .take()
            .ok_or(EngineError::EventChannelUnavailable)?;
        let state = Arc::clone(&self.state);
        let watch_root = self.watch_root.clone();
        let graph_worker_state = Arc::clone(&self.state);

        let graph_task =
            tokio::spawn(
                async move { run_graph_worker(graph_worker_state, graph_notify_rx).await },
            );

        let task = tokio::spawn(async move {
            info!("memori-core daemon started");

            let metadata = match state.vector_store.read_index_metadata().await {
                Ok(metadata) => metadata,
                Err(err) => {
                    error!(error = %err, "读取索引元数据失败，守护进程退出");
                    return Err(EngineError::Storage(err));
                }
            };

            if metadata.rebuild_state == RebuildState::Ready {
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
            } else {
                info!(
                    rebuild_state = metadata.rebuild_state.as_str(),
                    rebuild_reason = metadata.rebuild_reason.as_deref().unwrap_or(""),
                    "检测到索引版本不兼容，跳过旧缓存加载并准备全量重建"
                );
            }

            let runtime_cfg = { state.indexing_runtime.read().await.config.clone() };
            if let Some(root) = watch_root.clone()
                && runtime_cfg.mode != IndexingMode::Manual
                && is_within_schedule_window(&runtime_cfg)
            {
                if metadata.rebuild_state == RebuildState::Required
                    || metadata.rebuild_state == RebuildState::Rebuilding
                {
                    run_full_rebuild(
                        &state,
                        &root,
                        Some(&graph_notify_tx),
                        metadata
                            .rebuild_reason
                            .as_deref()
                            .unwrap_or("index_upgrade_required"),
                    )
                    .await?;
                } else {
                    {
                        let mut runtime = state.indexing_runtime.write().await;
                        runtime.phase = "scanning".to_string();
                        runtime.last_scan_at = Some(unix_now_secs());
                        runtime.last_error = None;
                    }

                    let existing_files =
                        collect_supported_text_files_recursively(root.clone()).await;
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
                        process_file_event(
                            &state,
                            &event,
                            Some(&graph_notify_tx),
                            watch_root.as_deref(),
                            false,
                        )
                        .await;
                    }

                    let mut runtime = state.indexing_runtime.write().await;
                    runtime.phase = "idle".to_string();
                    runtime.last_scan_at = Some(unix_now_secs());
                }
            }

            while let Some(event) = event_rx.recv().await {
                let (paused, cfg) = {
                    let runtime = state.indexing_runtime.read().await;
                    (runtime.paused, runtime.config.clone())
                };
                if paused || cfg.mode == IndexingMode::Manual || !is_within_schedule_window(&cfg) {
                    continue;
                }

                match event.kind {
                    WatchEventKind::Created
                    | WatchEventKind::Modified
                    | WatchEventKind::Renamed
                    | WatchEventKind::Removed => {
                        process_file_event(
                            &state,
                            &event,
                            Some(&graph_notify_tx),
                            watch_root.as_deref(),
                            false,
                        )
                        .await;
                    }
                }
            }

            info!("memori-core event channel closed, daemon exiting");
            Ok(())
        });

        self.daemon_task = Some(task);
        self.graph_worker_task = Some(graph_task);
        Ok(())
    }

    /// 关闭引擎：
    /// 1) 优先停止 memori-vault（关闭发送端）；
    /// 2) 等待 daemon 消费完剩余事件后退出。
    pub async fn shutdown(mut self) -> Result<(), EngineError> {
        self.graph_notify_tx.take();

        if let Some(memori_vault_handle) = self.memori_vault_handle.take() {
            memori_vault_handle.join().await?;
        }

        if let Some(daemon_task) = self.daemon_task.take() {
            daemon_task.await??;
        }
        if let Some(graph_worker_task) = self.graph_worker_task.take() {
            graph_worker_task.await??;
        }

        Ok(())
    }
}

fn analyze_query(query: &str) -> QueryAnalysis {
    let normalized_query = query.split_whitespace().collect::<Vec<_>>().join(" ");
    let raw_tokens = extract_query_tokens(&normalized_query);
    let mut lexical_terms = Vec::new();
    let mut document_terms = Vec::new();
    let mut filename_terms = Vec::new();
    let mut identifier_terms = Vec::new();
    let mut seen_lexical = HashMap::<String, ()>::new();
    let mut seen_document = HashMap::<String, ()>::new();
    let mut seen_filename = HashMap::<String, ()>::new();
    let mut seen_identifier = HashMap::<String, ()>::new();
    let mut flags = QueryFlags::default();

    for token in &raw_tokens {
        if token.chars().any(is_cjk) {
            flags.has_cjk = true;
        }
        if token.chars().any(|ch| ch.is_ascii_alphabetic())
            && token
                .chars()
                .any(|ch| ch.is_ascii_digit() || matches!(ch, '_' | '-' | '.' | '/' | '\\'))
        {
            flags.has_ascii_identifier = true;
        }
        if token
            .chars()
            .any(|ch| matches!(ch, '.' | '/' | '\\' | '_' | '-'))
        {
            flags.has_path_like_token = true;
        }

        for term in expand_query_token(token) {
            if is_english_stopword(&term) {
                continue;
            }
            if insert_unique_term(&mut seen_lexical, &mut lexical_terms, &term) {
                insert_unique_term(&mut seen_document, &mut document_terms, &term);
                if term.chars().any(|ch| ch.is_ascii_digit())
                    || term
                        .chars()
                        .any(|ch| matches!(ch, '.' | '/' | '\\' | '_' | '-'))
                {
                    insert_unique_term(&mut seen_filename, &mut filename_terms, &term);
                }
                if looks_like_identifier_term(&term, token) {
                    insert_unique_term(&mut seen_identifier, &mut identifier_terms, &term);
                }
            }
        }
    }

    if filename_terms.is_empty() {
        for term in &document_terms {
            if term.chars().any(is_cjk) || term.chars().any(|ch| ch.is_ascii_digit()) {
                insert_unique_term(&mut seen_filename, &mut filename_terms, term);
            }
        }
    }

    flags.token_count = lexical_terms.len();
    flags.is_lookup_like = is_lookup_like_query(&normalized_query)
        || flags.has_path_like_token
        || !filename_terms.is_empty()
        || !identifier_terms.is_empty();
    let query_intent = classify_query_intent(&normalized_query, &flags);

    QueryAnalysis {
        normalized_query: normalized_query.clone(),
        lexical_query: query_string_for_terms(&lexical_terms, &normalized_query),
        document_routing_terms: if document_terms.is_empty() {
            lexical_terms.clone()
        } else {
            document_terms
        },
        chunk_terms: lexical_terms,
        filename_like_terms: filename_terms,
        identifier_terms,
        query_intent,
        flags,
    }
}

fn extract_query_tokens(query: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();

    for ch in query.chars() {
        if is_query_token_char(ch) {
            current.push(ch);
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

fn expand_query_token(token: &str) -> Vec<String> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let mut expanded = Vec::new();
    let normalized = normalize_ascii_token(trimmed);
    if is_valid_query_term(&normalized) {
        expanded.push(normalized.clone());
    }

    if trimmed.chars().any(is_cjk) {
        let cjk_with_digits = trimmed
            .chars()
            .filter(|ch| is_cjk(*ch) || ch.is_ascii_digit())
            .collect::<String>();
        if is_valid_query_term(&cjk_with_digits) {
            expanded.push(cjk_with_digits.clone());
        }
        let pure_cjk = cjk_with_digits
            .chars()
            .filter(|ch| is_cjk(*ch))
            .collect::<String>();
        if is_valid_query_term(&pure_cjk) {
            expanded.push(pure_cjk.clone());
        }
        for phrase in extract_cjk_query_phrases(&cjk_with_digits) {
            if is_valid_query_term(&phrase) {
                expanded.push(phrase);
            }
        }
        for phrase in extract_cjk_query_phrases(&pure_cjk) {
            if is_valid_query_term(&phrase) {
                expanded.push(phrase);
            }
        }
    }

    if trimmed
        .chars()
        .any(|ch| matches!(ch, '.' | '/' | '\\' | '_' | '-'))
    {
        if let Some((stem, _ext)) = trimmed.rsplit_once('.') {
            let stem = normalize_ascii_token(stem);
            if is_valid_query_term(&stem) {
                expanded.push(stem);
            }
        }
        for part in trimmed.split(['.', '/', '\\', '_', '-']) {
            let normalized = normalize_ascii_token(part);
            if is_valid_query_term(&normalized) {
                expanded.push(normalized);
            }
        }
    }

    let mut deduped = Vec::new();
    let mut seen = HashMap::<String, ()>::new();
    for term in expanded {
        insert_unique_term(&mut seen, &mut deduped, &term);
    }
    deduped
}

fn extract_cjk_query_phrases(token: &str) -> Vec<String> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let mut phrases = Vec::new();
    if !is_cjk_question_phrase(trimmed) {
        phrases.push(trimmed.to_string());
    }

    for suffix in [
        "是什么",
        "是啥",
        "什么",
        "怎么",
        "如何",
        "为什么",
        "多少",
        "哪一个",
        "哪个",
        "哪里",
        "在哪",
        "谁",
    ] {
        if trimmed.ends_with(suffix) {
            let candidate = trimmed.trim_end_matches(suffix).trim().to_string();
            if candidate.chars().count() >= 2 {
                phrases.push(candidate);
            }
        }
    }

    phrases
}

fn is_cjk_question_phrase(token: &str) -> bool {
    matches!(
        token,
        "是什么"
            | "是啥"
            | "什么"
            | "怎么"
            | "如何"
            | "为什么"
            | "多少"
            | "哪一个"
            | "哪个"
            | "哪里"
            | "在哪"
            | "谁"
    )
}

fn normalize_ascii_token(token: &str) -> String {
    token
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphabetic() {
                ch.to_ascii_lowercase()
            } else {
                ch
            }
        })
        .collect::<String>()
}

fn is_valid_query_term(term: &str) -> bool {
    let trimmed = term.trim();
    if trimmed.is_empty() {
        return false;
    }
    if is_english_stopword(trimmed) {
        return false;
    }
    if trimmed.chars().any(is_cjk) {
        return trimmed.chars().count() >= 2;
    }
    if trimmed.chars().all(|ch| ch.is_ascii_digit()) {
        return !trimmed.is_empty();
    }
    trimmed.chars().count() >= 2
}

fn is_query_token_char(ch: char) -> bool {
    is_cjk(ch) || ch.is_ascii_alphanumeric() || matches!(ch, '.' | '/' | '\\' | '_' | '-')
}

fn looks_like_identifier_term(term: &str, raw_token: &str) -> bool {
    term.chars()
        .any(|ch| matches!(ch, '.' | '/' | '\\' | '_' | '-'))
        || term.chars().any(|ch| ch.is_ascii_digit())
        || raw_token
            .chars()
            .any(|ch| matches!(ch, '.' | '/' | '\\' | '_' | '-'))
        || raw_token.chars().any(|ch| ch.is_ascii_digit())
        || has_ascii_camel_case(raw_token)
}

fn has_ascii_camel_case(token: &str) -> bool {
    let chars = token.chars().collect::<Vec<_>>();
    chars.windows(2).any(|pair| {
        let [left, right] = pair else {
            return false;
        };
        left.is_ascii_lowercase() && right.is_ascii_uppercase()
    })
}

fn insert_unique_term(
    seen: &mut HashMap<String, ()>,
    target: &mut Vec<String>,
    term: &str,
) -> bool {
    let normalized = term.trim().to_string();
    if normalized.is_empty() || seen.contains_key(&normalized) {
        return false;
    }
    seen.insert(normalized.clone(), ());
    target.push(normalized);
    true
}

fn query_string_for_terms(terms: &[String], fallback: &str) -> String {
    if terms.is_empty() {
        fallback.trim().to_string()
    } else {
        terms.join(" ")
    }
}

fn is_english_stopword(term: &str) -> bool {
    matches!(
        term.trim().to_ascii_lowercase().as_str(),
        "a" | "an"
            | "the"
            | "is"
            | "are"
            | "was"
            | "were"
            | "what"
            | "which"
            | "who"
            | "when"
            | "where"
            | "why"
            | "how"
            | "do"
            | "does"
            | "did"
            | "can"
            | "could"
            | "should"
            | "would"
            | "will"
            | "to"
            | "of"
            | "in"
            | "on"
            | "for"
            | "from"
            | "by"
            | "with"
            | "and"
            | "or"
            | "my"
            | "your"
            | "me"
    )
}

fn query_flags_as_labels(flags: &QueryFlags) -> Vec<String> {
    let mut labels = Vec::new();
    if flags.has_cjk {
        labels.push("cjk".to_string());
    }
    if flags.has_ascii_identifier {
        labels.push("ascii_identifier".to_string());
    }
    if flags.has_path_like_token {
        labels.push("path_like".to_string());
    }
    if flags.is_lookup_like {
        labels.push("lookup_like".to_string());
    }
    labels.push(format!("token_count:{}", flags.token_count));
    labels
}

fn document_signal_query(analysis: &QueryAnalysis) -> String {
    let mut signal_terms = Vec::new();
    let mut seen = HashMap::<String, ()>::new();

    for term in &analysis.identifier_terms {
        insert_unique_term(&mut seen, &mut signal_terms, term);
    }
    for term in &analysis.filename_like_terms {
        insert_unique_term(&mut seen, &mut signal_terms, term);
    }
    for term in &analysis.document_routing_terms {
        insert_unique_term(&mut seen, &mut signal_terms, term);
    }

    if signal_terms.is_empty() && analysis.flags.is_lookup_like {
        analysis.normalized_query.clone()
    } else {
        signal_terms.join(" ")
    }
}

fn merge_document_candidates(
    analysis: &QueryAnalysis,
    exact_docs: Vec<memori_storage::DocumentSignalMatch>,
    strict_docs: Vec<memori_storage::FtsDocumentMatch>,
    broad_docs: Vec<memori_storage::FtsDocumentMatch>,
) -> Vec<DocumentCandidate> {
    let mut merged = HashMap::<String, DocumentCandidate>::new();
    let implementation_lookup = is_implementation_lookup(analysis);

    for (index, doc) in exact_docs.into_iter().enumerate() {
        let is_code_document = is_code_document_path(&doc.relative_path);
        let is_exact_path = doc.matched_fields.iter().any(|field| field == "exact_path");
        let is_exact_symbol = doc
            .matched_fields
            .iter()
            .any(|field| field == "exact_symbol");
        let is_exact = is_exact_path || is_exact_symbol;
        let is_filename = doc
            .matched_fields
            .iter()
            .any(|field| matches!(field.as_str(), "exact_path" | "file_name" | "relative_path"));
        let weight = if is_exact_path {
            6.0
        } else if is_exact_symbol {
            5.0
        } else {
            3.0
        };
        let score = weight / (RRF_K + (index + 1) as f64);
        merged
            .entry(doc.file_path.clone())
            .and_modify(|entry| {
                entry.exact_signal_score =
                    entry.exact_signal_score.max(is_exact.then_some(doc.score));
                entry.exact_path_score = entry
                    .exact_path_score
                    .max(is_exact_path.then_some(doc.score));
                entry.exact_symbol_score = entry
                    .exact_symbol_score
                    .max(is_exact_symbol.then_some(doc.score));
                entry.document_filename_score = entry
                    .document_filename_score
                    .max(is_filename.then_some(doc.score));
                entry.has_exact_signal |= is_exact;
                entry.has_exact_path_signal |= is_exact_path;
                entry.has_exact_symbol_signal |= is_exact_symbol;
                entry.has_filename_signal |= is_filename;
                entry.document_final_score += score;
            })
            .or_insert(DocumentCandidate {
                file_path: doc.file_path,
                relative_path: doc.relative_path,
                file_name: doc.file_name,
                is_code_document,
                document_reason: if is_exact_path {
                    "exact_path".to_string()
                } else if is_exact_symbol {
                    "exact_symbol".to_string()
                } else {
                    "filename".to_string()
                },
                document_rank: index + 1,
                document_raw_score: None,
                exact_signal_score: is_exact.then_some(doc.score),
                exact_path_score: is_exact_path.then_some(doc.score),
                exact_symbol_score: is_exact_symbol.then_some(doc.score),
                document_filename_score: is_filename.then_some(doc.score),
                document_final_score: score,
                has_exact_signal: is_exact,
                has_exact_path_signal: is_exact_path,
                has_exact_symbol_signal: is_exact_symbol,
                has_filename_signal: is_filename,
                has_strict_lexical: false,
                has_broad_lexical: false,
            });
    }

    for (index, doc) in strict_docs.into_iter().enumerate() {
        let is_code_document = is_code_document_path(&doc.relative_path);
        let score = 3.0 / (RRF_K + (index + 1) as f64);
        merged
            .entry(doc.file_path.clone())
            .and_modify(|entry| {
                entry.document_raw_score = Some(
                    entry
                        .document_raw_score
                        .map(|current| current.max(doc.score))
                        .unwrap_or(doc.score),
                );
                entry.has_strict_lexical = true;
                entry.document_final_score += score;
            })
            .or_insert(DocumentCandidate {
                file_path: doc.file_path,
                relative_path: doc.relative_path,
                file_name: doc.file_name,
                is_code_document,
                document_reason: "lexical_strict".to_string(),
                document_rank: index + 1,
                document_raw_score: Some(doc.score),
                exact_signal_score: None,
                exact_path_score: None,
                exact_symbol_score: None,
                document_filename_score: None,
                document_final_score: score,
                has_exact_signal: false,
                has_exact_path_signal: false,
                has_exact_symbol_signal: false,
                has_filename_signal: false,
                has_strict_lexical: true,
                has_broad_lexical: false,
            });
    }

    for (index, doc) in broad_docs.into_iter().enumerate() {
        let is_code_document = is_code_document_path(&doc.relative_path);
        let score = if implementation_lookup && is_routing_noise_document(&doc.relative_path) {
            0.35 / (RRF_K + (index + 1) as f64)
        } else {
            1.0 / (RRF_K + (index + 1) as f64)
        };
        merged
            .entry(doc.file_path.clone())
            .and_modify(|entry| {
                if entry.document_raw_score.is_none() {
                    entry.document_raw_score = Some(doc.score);
                }
                entry.has_broad_lexical = true;
                entry.document_final_score += score;
            })
            .or_insert(DocumentCandidate {
                file_path: doc.file_path,
                relative_path: doc.relative_path,
                file_name: doc.file_name,
                is_code_document,
                document_reason: "lexical_broad".to_string(),
                document_rank: index + 1,
                document_raw_score: Some(doc.score),
                exact_signal_score: None,
                exact_path_score: None,
                exact_symbol_score: None,
                document_filename_score: None,
                document_final_score: score,
                has_exact_signal: false,
                has_exact_path_signal: false,
                has_exact_symbol_signal: false,
                has_filename_signal: false,
                has_strict_lexical: false,
                has_broad_lexical: true,
            });
    }

    let mut docs = merged.into_values().collect::<Vec<_>>();
    for doc in &mut docs {
        doc.document_reason = if doc.has_exact_path_signal {
            "exact_path".to_string()
        } else if doc.has_exact_symbol_signal {
            "exact_symbol".to_string()
        } else if doc.has_exact_signal && (doc.has_strict_lexical || doc.has_broad_lexical) {
            "mixed".to_string()
        } else if doc.has_filename_signal && (doc.has_strict_lexical || doc.has_broad_lexical) {
            "mixed".to_string()
        } else if doc.has_strict_lexical {
            "lexical_strict".to_string()
        } else if doc.has_filename_signal {
            "filename".to_string()
        } else {
            "lexical_broad".to_string()
        };
    }
    docs.sort_by(|a, b| {
        document_reason_priority(&b.document_reason, implementation_lookup)
            .cmp(&document_reason_priority(
                &a.document_reason,
                implementation_lookup,
            ))
            .then_with(|| {
                document_type_priority(b.is_code_document, implementation_lookup).cmp(
                    &document_type_priority(a.is_code_document, implementation_lookup),
                )
            })
            .then_with(|| b.document_final_score.total_cmp(&a.document_final_score))
            .then_with(|| {
                b.exact_path_score
                    .unwrap_or_default()
                    .cmp(&a.exact_path_score.unwrap_or_default())
            })
            .then_with(|| {
                b.exact_symbol_score
                    .unwrap_or_default()
                    .cmp(&a.exact_symbol_score.unwrap_or_default())
            })
            .then_with(|| {
                b.exact_signal_score
                    .unwrap_or_default()
                    .cmp(&a.exact_signal_score.unwrap_or_default())
            })
            .then_with(|| {
                b.document_filename_score
                    .unwrap_or_default()
                    .cmp(&a.document_filename_score.unwrap_or_default())
            })
            .then_with(|| {
                b.document_raw_score
                    .unwrap_or_default()
                    .total_cmp(&a.document_raw_score.unwrap_or_default())
            })
            .then_with(|| a.relative_path.cmp(&b.relative_path))
    });
    for (index, doc) in docs.iter_mut().enumerate() {
        doc.document_rank = index + 1;
    }
    docs
}

fn is_code_document_path(relative_path: &str) -> bool {
    relative_path
        .rsplit_once('.')
        .map(|(_, ext)| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "rs" | "ts" | "tsx" | "js" | "jsx"
            )
        })
        .unwrap_or(false)
}

fn document_type_priority(is_code_document: bool, implementation_lookup: bool) -> u8 {
    if implementation_lookup {
        u8::from(is_code_document)
    } else {
        u8::from(!is_code_document)
    }
}

fn document_reason_priority(reason: &str, implementation_lookup: bool) -> u8 {
    if implementation_lookup {
        match reason {
            "exact_path" => 6,
            "exact_symbol" => 5,
            "mixed" => 4,
            "filename" => 3,
            "lexical_strict" => 2,
            "lexical_broad" => 1,
            _ => 0,
        }
    } else {
        match reason {
            "scope" => 7,
            "exact_path" => 6,
            "mixed" => 5,
            "filename" => 4,
            "lexical_strict" => 3,
            "exact_symbol" => 2,
            "lexical_broad" => 1,
            _ => 0,
        }
    }
}

fn is_implementation_lookup(analysis: &QueryAnalysis) -> bool {
    matches!(analysis.query_intent, QueryIntent::RepoLookup)
        && (analysis.flags.has_ascii_identifier
            || analysis.flags.has_path_like_token
            || !analysis.identifier_terms.is_empty())
}

fn is_routing_noise_document(relative_path: &str) -> bool {
    matches!(
        relative_path.to_ascii_lowercase().as_str(),
        "readme.md" | "docs/plan.md" | "docs/tutorial.md"
    )
}

fn merge_chunk_evidence(
    analysis: &QueryAnalysis,
    candidate_docs: &[DocumentCandidate],
    strict_lexical_matches: Vec<memori_storage::FtsChunkMatch>,
    lexical_matches: Vec<memori_storage::FtsChunkMatch>,
    dense_matches: Vec<(DocumentChunk, f32)>,
) -> Vec<MergedEvidence> {
    let mut doc_rank_by_path = HashMap::new();
    for doc in candidate_docs {
        doc_rank_by_path.insert(
            doc.file_path.clone(),
            (
                doc.relative_path.clone(),
                doc.document_reason.clone(),
                doc.document_rank,
                doc.document_raw_score,
                doc.has_exact_signal,
                doc.has_filename_signal,
                doc.has_strict_lexical,
            ),
        );
    }

    let mut merged = HashMap::<(String, usize), MergedEvidence>::new();
    for (index, item) in strict_lexical_matches.into_iter().enumerate() {
        let Some((
            relative_path,
            document_reason,
            document_rank,
            document_raw_score,
            document_has_exact_signal,
            document_has_filename_signal,
            document_has_strict_lexical,
        )) = doc_rank_by_path.get(&item.file_path).cloned()
        else {
            continue;
        };
        let key = (item.file_path.clone(), item.chunk_index);
        let final_score = 2.0 / (RRF_K + (index + 1) as f64);
        merged
            .entry(key)
            .and_modify(|entry| {
                entry.lexical_strict_rank = Some(index + 1);
                entry.lexical_raw_score = Some(
                    entry
                        .lexical_raw_score
                        .map(|current| current.max(item.score))
                        .unwrap_or(item.score),
                );
                entry.final_score += final_score;
            })
            .or_insert(MergedEvidence {
                chunk: DocumentChunk {
                    file_path: PathBuf::from(&item.file_path),
                    content: item.content.clone(),
                    chunk_index: item.chunk_index,
                    heading_path: item.heading_path.clone(),
                    block_kind: parse_block_kind(&item.block_kind),
                },
                relative_path,
                document_reason,
                document_rank,
                document_raw_score,
                document_has_exact_signal,
                document_has_filename_signal,
                document_has_strict_lexical,
                lexical_strict_rank: Some(index + 1),
                lexical_broad_rank: None,
                lexical_raw_score: Some(item.score),
                dense_rank: None,
                dense_raw_score: None,
                final_score,
            });
    }

    for (index, item) in lexical_matches.into_iter().enumerate() {
        let Some((
            relative_path,
            document_reason,
            document_rank,
            document_raw_score,
            document_has_exact_signal,
            document_has_filename_signal,
            document_has_strict_lexical,
        )) = doc_rank_by_path.get(&item.file_path).cloned()
        else {
            continue;
        };
        let key = (item.file_path.clone(), item.chunk_index);
        let final_score = 1.0 / (RRF_K + (index + 1) as f64);
        merged
            .entry(key)
            .and_modify(|entry| {
                entry.lexical_broad_rank = Some(index + 1);
                entry.lexical_raw_score = Some(
                    entry
                        .lexical_raw_score
                        .map(|current| current.max(item.score))
                        .unwrap_or(item.score),
                );
                entry.final_score += final_score;
            })
            .or_insert(MergedEvidence {
                chunk: DocumentChunk {
                    file_path: PathBuf::from(&item.file_path),
                    content: item.content.clone(),
                    chunk_index: item.chunk_index,
                    heading_path: item.heading_path.clone(),
                    block_kind: parse_block_kind(&item.block_kind),
                },
                relative_path,
                document_reason,
                document_rank,
                document_raw_score,
                document_has_exact_signal,
                document_has_filename_signal,
                document_has_strict_lexical,
                lexical_strict_rank: None,
                lexical_broad_rank: Some(index + 1),
                lexical_raw_score: Some(item.score),
                dense_rank: None,
                dense_raw_score: None,
                final_score,
            });
    }

    for (index, (chunk, dense_score)) in dense_matches.into_iter().enumerate() {
        let file_path = chunk.file_path.to_string_lossy().to_string();
        let Some((
            relative_path,
            document_reason,
            document_rank,
            document_raw_score,
            document_has_exact_signal,
            document_has_filename_signal,
            document_has_strict_lexical,
        )) = doc_rank_by_path.get(&file_path).cloned()
        else {
            continue;
        };
        let key = (file_path, chunk.chunk_index);
        let final_score = 1.0 / (RRF_K + (index + 1) as f64);
        merged
            .entry(key)
            .and_modify(|entry| {
                entry.dense_rank = Some(index + 1);
                entry.dense_raw_score = Some(dense_score);
                entry.final_score += final_score;
            })
            .or_insert(MergedEvidence {
                chunk,
                relative_path,
                document_reason,
                document_rank,
                document_raw_score,
                document_has_exact_signal,
                document_has_filename_signal,
                document_has_strict_lexical,
                lexical_strict_rank: None,
                lexical_broad_rank: None,
                lexical_raw_score: None,
                dense_rank: Some(index + 1),
                dense_raw_score: Some(dense_score),
                final_score,
            });
    }

    for item in merged.values_mut() {
        if has_any_chunk_lexical(item) {
            continue;
        }
        let Some((is_strict, signal_score)) = direct_chunk_lexical_signal(analysis, &item.chunk)
        else {
            continue;
        };
        if is_strict {
            item.lexical_strict_rank = Some(DEFAULT_CHUNK_CANDIDATE_K + item.document_rank);
            item.lexical_raw_score = Some(signal_score);
            item.final_score += 0.75 / (RRF_K + DEFAULT_CHUNK_CANDIDATE_K as f64);
        } else {
            item.lexical_broad_rank = Some(DEFAULT_CHUNK_CANDIDATE_K + item.document_rank);
            item.lexical_raw_score = Some(signal_score);
            item.final_score += 0.35 / (RRF_K + DEFAULT_CHUNK_CANDIDATE_K as f64);
        }
    }

    let mut items = merged.into_values().collect::<Vec<_>>();
    items.sort_by(|a, b| {
        a.document_rank
            .cmp(&b.document_rank)
            .then_with(|| b.final_score.total_cmp(&a.final_score))
            .then_with(|| a.chunk.chunk_index.cmp(&b.chunk.chunk_index))
    });
    items
}

fn should_refuse_for_insufficient_evidence(
    analysis: &QueryAnalysis,
    evidence: &[MergedEvidence],
) -> bool {
    if evidence.is_empty() {
        return true;
    }
    if matches!(
        analysis.query_intent,
        QueryIntent::ExternalFact | QueryIntent::SecretRequest | QueryIntent::MissingFileLookup
    ) {
        return true;
    }
    if should_force_missing_file_lookup(analysis, evidence) {
        return true;
    }

    let Some(top) = evidence.first() else {
        return true;
    };
    let top_doc_path = top.chunk.file_path.to_string_lossy().to_string();
    let top_doc_evidence = evidence
        .iter()
        .filter(|item| item.chunk.file_path.to_string_lossy() == top_doc_path)
        .collect::<Vec<_>>();
    let top_doc_count = top_doc_evidence.len();
    let top_doc_any_lexical = top_doc_evidence
        .iter()
        .filter(|item| has_any_chunk_lexical(item))
        .count();
    let top_doc_strict_lexical = top_doc_evidence
        .iter()
        .filter(|item| item.lexical_strict_rank.is_some())
        .count();
    let query_is_long =
        analysis.normalized_query.chars().count() >= 8 || analysis.flags.token_count >= 3;

    if top.lexical_strict_rank.is_some() && top_doc_count >= 2 && has_strong_document_signal(top) {
        return false;
    }

    if top.document_rank == 1
        && has_strong_document_signal(top)
        && has_any_chunk_lexical(top)
        && evidence.len() >= 2
    {
        return false;
    }

    if analysis.flags.is_lookup_like
        && top.document_has_filename_signal
        && has_any_chunk_lexical(top)
        && top_doc_count >= 2
    {
        return false;
    }

    if !analysis.flags.is_lookup_like && top_doc_any_lexical >= 2 && top.document_rank <= 3 {
        return false;
    }

    if top_doc_count >= 2 && top_doc_strict_lexical >= 1 && has_strong_document_signal(top) {
        return false;
    }

    !has_any_chunk_lexical(top) && top.dense_rank.is_some() && query_is_long
}

fn has_any_chunk_lexical(item: &MergedEvidence) -> bool {
    item.lexical_strict_rank.is_some() || item.lexical_broad_rank.is_some()
}

fn has_strong_document_signal(item: &MergedEvidence) -> bool {
    item.document_has_exact_signal
        || item.document_has_filename_signal
        || item.document_has_strict_lexical
        || item.document_reason == "scope"
}

fn direct_chunk_lexical_signal(
    analysis: &QueryAnalysis,
    chunk: &DocumentChunk,
) -> Option<(bool, f64)> {
    let content = chunk.content.to_ascii_lowercase();
    let heading = chunk.heading_path.join(" / ").to_ascii_lowercase();
    let file_path = chunk.file_path.to_string_lossy().to_ascii_lowercase();
    let mut strict_hits = 0_u32;
    let mut broad_hits = 0_u32;

    for term in analysis
        .identifier_terms
        .iter()
        .chain(analysis.filename_like_terms.iter())
    {
        if chunk_text_contains_term(&content, &heading, &file_path, term) {
            strict_hits += 1;
        }
    }

    for term in &analysis.chunk_terms {
        if !is_direct_lexical_support_term(term) {
            continue;
        }
        if chunk_text_contains_term(&content, &heading, &file_path, term) {
            if term.chars().any(is_cjk)
                || term.chars().any(|ch| ch.is_ascii_digit())
                || term
                    .chars()
                    .any(|ch| matches!(ch, '.' | '/' | '\\' | '_' | '-'))
            {
                strict_hits += 1;
            } else {
                broad_hits += 1;
            }
        }
    }

    if strict_hits > 0 {
        Some((true, strict_hits as f64))
    } else if broad_hits > 0 {
        Some((false, broad_hits as f64))
    } else {
        None
    }
}

fn is_direct_lexical_support_term(term: &str) -> bool {
    term.chars().any(is_cjk)
        || term.chars().any(|ch| ch.is_ascii_digit())
        || term
            .chars()
            .any(|ch| matches!(ch, '.' | '/' | '\\' | '_' | '-'))
        || term.chars().count() >= 6
}

fn chunk_text_contains_term(content: &str, heading: &str, file_path: &str, term: &str) -> bool {
    let needle = term.trim().to_ascii_lowercase();
    !needle.is_empty()
        && (content.contains(&needle) || heading.contains(&needle) || file_path.contains(&needle))
}

fn should_force_missing_file_lookup(analysis: &QueryAnalysis, evidence: &[MergedEvidence]) -> bool {
    if !analysis.flags.is_lookup_like {
        return false;
    }

    let has_document_signal = evidence.iter().any(has_strong_document_signal);
    let lower = analysis.normalized_query.to_ascii_lowercase();
    let asks_for_content = is_direct_content_request_query(&analysis.normalized_query);
    let mentions_scope_exclusion = lower.contains("scope")
        && (analysis.normalized_query.contains("不包含")
            || lower.contains("not include")
            || lower.contains("outside scope"));
    let has_named_file_term = analysis
        .identifier_terms
        .iter()
        .chain(analysis.filename_like_terms.iter())
        .any(|term| is_named_file_lookup_term(term));
    let has_requested_path_match = evidence_matches_requested_file(analysis, evidence);

    (mentions_scope_exclusion || (asks_for_content && has_named_file_term))
        && !has_requested_path_match
        || (!has_document_signal
            && asks_for_content
            && analysis
                .identifier_terms
                .iter()
                .any(|term| term.contains('.') || term.contains('/') || term.contains('\\')))
}

fn is_direct_content_request_query(query: &str) -> bool {
    let lower = query.to_ascii_lowercase();
    [
        "summarize",
        "summary",
        "content",
        "contents",
        "帮我总结",
        "总结",
        "概括",
        "内容",
        "解释",
        "from my vault",
    ]
    .iter()
    .any(|marker| lower.contains(marker) || query.contains(marker))
}

fn should_mark_missing_file_lookup_intent(analysis: &QueryAnalysis) -> bool {
    is_direct_content_request_query(&analysis.normalized_query)
        && analysis
            .identifier_terms
            .iter()
            .chain(analysis.filename_like_terms.iter())
            .any(|term| is_named_file_lookup_term(term))
}

fn is_named_file_lookup_term(term: &str) -> bool {
    term.chars().any(is_cjk)
        || term.chars().any(|ch| ch.is_ascii_digit())
        || term
            .chars()
            .any(|ch| matches!(ch, '.' | '/' | '\\' | '_' | '-'))
}

fn evidence_matches_requested_file(analysis: &QueryAnalysis, evidence: &[MergedEvidence]) -> bool {
    let requested_terms = analysis
        .identifier_terms
        .iter()
        .chain(analysis.filename_like_terms.iter())
        .filter(|term| is_named_file_lookup_term(term))
        .map(|term| term.trim().to_ascii_lowercase())
        .collect::<Vec<_>>();

    if requested_terms.is_empty() {
        return false;
    }

    evidence.iter().any(|item| {
        let relative = item.relative_path.to_ascii_lowercase();
        let file_name = item
            .chunk
            .file_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        requested_terms.iter().any(|term| {
            relative.contains(term)
                || file_name == *term
                || file_name
                    .strip_suffix(".md")
                    .is_some_and(|stem| stem == term)
        })
    })
}

fn build_citations(evidence: &[MergedEvidence]) -> Vec<CitationItem> {
    evidence
        .iter()
        .enumerate()
        .map(|(index, item)| CitationItem {
            index: index + 1,
            file_path: item.chunk.file_path.to_string_lossy().to_string(),
            relative_path: item.relative_path.clone(),
            chunk_index: item.chunk.chunk_index,
            heading_path: item.chunk.heading_path.clone(),
            excerpt: build_reference_excerpt(&item.chunk.file_path, &item.chunk.content),
        })
        .collect()
}

fn build_evidence_items(evidence: &[MergedEvidence]) -> Vec<EvidenceItem> {
    evidence
        .iter()
        .enumerate()
        .map(|(index, item)| EvidenceItem {
            file_path: item.chunk.file_path.to_string_lossy().to_string(),
            relative_path: item.relative_path.clone(),
            chunk_index: item.chunk.chunk_index,
            heading_path: item.chunk.heading_path.clone(),
            block_kind: block_kind_label(item.chunk.block_kind).to_string(),
            document_reason: item.document_reason.clone(),
            reason: evidence_reason(item).to_string(),
            document_rank: item.document_rank,
            chunk_rank: index + 1,
            document_raw_score: item.document_raw_score,
            lexical_raw_score: item.lexical_raw_score,
            dense_raw_score: item.dense_raw_score,
            final_score: item.final_score,
            content: item.chunk.content.clone(),
        })
        .collect()
}

fn build_merged_evidence_from_items(items: &[EvidenceItem]) -> Vec<MergedEvidence> {
    items
        .iter()
        .map(|item| MergedEvidence {
            chunk: DocumentChunk {
                file_path: PathBuf::from(&item.file_path),
                content: item.content.clone(),
                chunk_index: item.chunk_index,
                heading_path: item.heading_path.clone(),
                block_kind: parse_block_kind(&item.block_kind),
            },
            relative_path: item.relative_path.clone(),
            document_reason: item.document_reason.clone(),
            document_rank: item.document_rank,
            document_raw_score: item.document_raw_score,
            document_has_exact_signal: matches!(
                item.document_reason.as_str(),
                "exact_path" | "exact_symbol"
            ),
            document_has_filename_signal: matches!(
                item.document_reason.as_str(),
                "filename" | "mixed"
            ),
            document_has_strict_lexical: matches!(
                item.document_reason.as_str(),
                "lexical_strict" | "mixed"
            ),
            lexical_strict_rank: matches!(item.reason.as_str(), "lexical_strict" | "mixed")
                .then_some(item.chunk_rank),
            lexical_broad_rank: (item.reason == "lexical_broad").then_some(item.chunk_rank),
            lexical_raw_score: item.lexical_raw_score,
            dense_rank: matches!(item.reason.as_str(), "dense" | "mixed")
                .then_some(item.chunk_rank),
            dense_raw_score: item.dense_raw_score,
            final_score: item.final_score,
        })
        .collect()
}

fn prepare_query_for_retrieval(question: &str) -> QueryPreparation {
    let query_analysis_started_at = Instant::now();
    let analysis = analyze_query(question);
    let mut metrics = RetrievalMetrics::default();
    metrics.query_analysis_ms = elapsed_ms_u64(query_analysis_started_at);
    metrics.query_flags = query_flags_as_labels(&analysis.flags);
    metrics
        .query_flags
        .push(format!("intent:{}", analysis.query_intent.as_str()));
    if !analysis.identifier_terms.is_empty() {
        metrics.query_flags.push(format!(
            "identifier_terms:{}",
            analysis.identifier_terms.len()
        ));
    }
    if !analysis.filename_like_terms.is_empty() {
        metrics.query_flags.push(format!(
            "filename_terms:{}",
            analysis.filename_like_terms.len()
        ));
    }
    QueryPreparation { analysis, metrics }
}

pub fn build_query_terms_for_offline_embedding(query: &str) -> Vec<String> {
    let analysis = analyze_query(query);
    let mut terms = Vec::new();
    terms.extend(analysis.chunk_terms);
    terms.extend(analysis.identifier_terms);
    terms.extend(analysis.filename_like_terms);
    if !analysis.normalized_query.is_empty() {
        terms.push(analysis.normalized_query);
    }
    if !analysis.lexical_query.is_empty() {
        terms.push(analysis.lexical_query);
    }

    let mut seen = std::collections::HashSet::new();
    terms.retain(|term| {
        let normalized = term.trim().to_ascii_lowercase();
        !normalized.is_empty() && seen.insert(normalized)
    });
    terms
}

fn build_text_context_from_evidence(evidence: &[MergedEvidence]) -> String {
    let mut parts = Vec::with_capacity(evidence.len());
    for (index, item) in evidence.iter().enumerate() {
        let heading = if item.chunk.heading_path.is_empty() {
            String::new()
        } else {
            format!("标题路径: {}\n", item.chunk.heading_path.join(" > "))
        };
        parts.push(format!(
            "片段#{display_index}\n来源: {path}\n相对路径: {relative_path}\n块序号: {chunk_index}\n块类型: {block_kind}\n文档排序: #{document_rank}\n文档命中原因: {document_reason}\n片段排序分数: {score:.6}\n命中原因: {reason}\n{heading}内容:\n{content}",
            display_index = index + 1,
            path = item.chunk.file_path.display(),
            relative_path = item.relative_path,
            chunk_index = item.chunk.chunk_index,
            block_kind = block_kind_label(item.chunk.block_kind),
            document_rank = item.document_rank,
            document_reason = &item.document_reason,
            score = item.final_score,
            reason = evidence_reason(item),
            heading = heading,
            content = item.chunk.content,
        ));
    }
    parts.join("\n\n")
}

fn build_reference_excerpt(file_path: &Path, chunk_content: &str) -> String {
    const TARGET_EXCERPT_CHARS: usize = 1600;

    let Ok(raw) = std::fs::read_to_string(file_path) else {
        return chunk_content.to_string();
    };

    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let paragraphs = normalized
        .split("\n\n")
        .map(str::trim)
        .filter(|paragraph| !paragraph.is_empty())
        .collect::<Vec<_>>();
    if paragraphs.is_empty() {
        return chunk_content.to_string();
    }

    let chunk_normalized = chunk_content.trim();
    let anchor = chunk_normalized
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && line.chars().count() >= 8)
        .unwrap_or(chunk_normalized);
    let paragraph_index = paragraphs
        .iter()
        .position(|paragraph| paragraph.contains(chunk_normalized))
        .or_else(|| {
            paragraphs
                .iter()
                .position(|paragraph| paragraph.contains(anchor))
        });

    let Some(index) = paragraph_index else {
        return chunk_content.to_string();
    };

    let mut start = index;
    let mut end = index + 1;
    let mut total_chars = paragraphs[index].chars().count();
    while total_chars < TARGET_EXCERPT_CHARS && (start > 0 || end < paragraphs.len()) {
        let prev_len = if start > 0 {
            paragraphs[start - 1].chars().count()
        } else {
            0
        };
        let next_len = if end < paragraphs.len() {
            paragraphs[end].chars().count()
        } else {
            0
        };
        if next_len >= prev_len && end < paragraphs.len() {
            total_chars += next_len;
            end += 1;
            continue;
        }
        if start > 0 {
            start -= 1;
            total_chars += prev_len;
            continue;
        }
        if end < paragraphs.len() {
            total_chars += next_len;
            end += 1;
        }
    }

    paragraphs[start..end].join("\n\n")
}

fn build_answer_question(query: &str, lang: Option<&str>) -> String {
    match normalize_language(lang) {
        Some("zh-CN") => format!("{query}\n\n请仅使用中文回答。"),
        Some("en-US") => format!("{query}\n\nPlease answer in English only."),
        _ => query.to_string(),
    }
}

fn normalize_language(lang: Option<&str>) -> Option<&'static str> {
    let lang = lang?;
    let lower = lang.trim().to_ascii_lowercase();
    if lower.starts_with("zh") {
        Some("zh-CN")
    } else if lower.starts_with("en") {
        Some("en-US")
    } else {
        None
    }
}

fn parse_block_kind(value: &str) -> memori_parser::ChunkBlockKind {
    match value.trim().to_ascii_lowercase().as_str() {
        "heading" => memori_parser::ChunkBlockKind::Heading,
        "list" => memori_parser::ChunkBlockKind::List,
        "code_block" => memori_parser::ChunkBlockKind::CodeBlock,
        "table" => memori_parser::ChunkBlockKind::Table,
        "quote" => memori_parser::ChunkBlockKind::Quote,
        "html" => memori_parser::ChunkBlockKind::Html,
        "thematic_break" => memori_parser::ChunkBlockKind::ThematicBreak,
        "mixed" => memori_parser::ChunkBlockKind::Mixed,
        _ => memori_parser::ChunkBlockKind::Paragraph,
    }
}

fn block_kind_label(kind: memori_parser::ChunkBlockKind) -> &'static str {
    match kind {
        memori_parser::ChunkBlockKind::Heading => "heading",
        memori_parser::ChunkBlockKind::Paragraph => "paragraph",
        memori_parser::ChunkBlockKind::List => "list",
        memori_parser::ChunkBlockKind::CodeBlock => "code_block",
        memori_parser::ChunkBlockKind::Table => "table",
        memori_parser::ChunkBlockKind::Quote => "quote",
        memori_parser::ChunkBlockKind::Html => "html",
        memori_parser::ChunkBlockKind::ThematicBreak => "thematic_break",
        memori_parser::ChunkBlockKind::Mixed => "mixed",
    }
}

fn evidence_reason(item: &MergedEvidence) -> &'static str {
    let has_strict = item.lexical_strict_rank.is_some();
    let has_broad = item.lexical_broad_rank.is_some();
    match (has_strict || has_broad, item.dense_rank.is_some()) {
        (true, true) => "mixed",
        (true, false) if has_strict => "lexical_strict",
        (true, false) => "lexical_broad",
        (false, true) => "dense",
        (false, false) => "unknown",
    }
}

fn classify_query_intent(query: &str, flags: &QueryFlags) -> QueryIntent {
    if is_secret_request_query(query) {
        return QueryIntent::SecretRequest;
    }
    if is_external_fact_query(query) {
        return QueryIntent::ExternalFact;
    }
    if flags.is_lookup_like {
        QueryIntent::RepoLookup
    } else {
        QueryIntent::RepoQuestion
    }
}

fn is_external_fact_query(query: &str) -> bool {
    let lower = query.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return false;
    }

    let role_fact_patterns = ["ceo of", "president of", "capital of", "founder of"];
    let time_sensitive_patterns = [
        "price today",
        "stock price",
        "bitcoin price",
        "btc price",
        "weather today",
        "news today",
        "today's news",
    ];

    role_fact_patterns
        .iter()
        .any(|pattern| lower.contains(pattern))
        || time_sensitive_patterns
            .iter()
            .any(|pattern| lower.contains(pattern))
        || query.contains("今天比特币价格")
        || query.contains("今天新闻")
}

fn is_secret_request_query(query: &str) -> bool {
    let lower = query.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return false;
    }

    let sensitive_markers = [
        "api key",
        "apikey",
        "secret",
        "password",
        "credential",
        "credentials",
        "token",
        "密钥",
        "密码",
        "凭据",
    ];
    let request_markers = [
        "hidden",
        "show",
        "reveal",
        "export",
        "dump",
        "what is",
        "local settings",
        "显示",
        "导出",
        "隐藏",
        "本地设置",
    ];

    sensitive_markers
        .iter()
        .any(|marker| lower.contains(marker) || query.contains(marker))
        && request_markers
            .iter()
            .any(|marker| lower.contains(marker) || query.contains(marker))
}

fn is_lookup_like_query(query: &str) -> bool {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.contains('.')
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.contains('_')
    {
        return true;
    }
    if trimmed.chars().any(|ch| ch.is_ascii_digit()) && query_token_count(trimmed) <= 6 {
        return true;
    }
    query_token_count(trimmed) <= 3 && trimmed.chars().count() <= 48
}

fn query_token_count(query: &str) -> usize {
    let mut count = 0;
    let mut in_token = false;
    for ch in query.chars() {
        let is_token = ch.is_alphanumeric() || is_cjk(ch);
        if is_token && !in_token {
            count += 1;
        }
        in_token = is_token;
    }
    count
}

fn is_cjk(ch: char) -> bool {
    ('\u{4E00}'..='\u{9FFF}').contains(&ch)
        || ('\u{3400}'..='\u{4DBF}').contains(&ch)
        || ('\u{3040}'..='\u{30FF}').contains(&ch)
}

fn elapsed_ms_u64(started_at: Instant) -> u64 {
    started_at
        .elapsed()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn resolve_db_path() -> Result<PathBuf, EngineError> {
    if let Ok(path) = std::env::var(MEMORI_DB_PATH_ENV) {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }

    if let Some(data_dir) = dirs::data_dir() {
        // Stable per-user location for desktop/server deployments.
        // Example (Windows): %APPDATA%/Memori-Vault/.memori.db
        // Example (Linux): ~/.local/share/Memori-Vault/.memori.db
        return Ok(data_dir.join("Memori-Vault").join(DEFAULT_DB_FILE_NAME));
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

async fn process_file_event(
    state: &Arc<AppState>,
    event: &WatchEvent,
    graph_notify_tx: Option<&mpsc::Sender<()>>,
    watch_root: Option<&std::path::Path>,
    allow_rebuild_write: bool,
) {
    if !allow_rebuild_write {
        match state.vector_store.read_index_metadata().await {
            Ok(metadata) if metadata.rebuild_state != RebuildState::Ready => {
                debug!(
                    path = %event.path.display(),
                    rebuild_state = metadata.rebuild_state.as_str(),
                    "索引当前不处于 ready 状态，已跳过文件事件写入"
                );
                return;
            }
            Ok(_) => {}
            Err(err) => {
                warn!(
                    path = %event.path.display(),
                    error = %err,
                    "读取索引元数据失败，已跳过文件事件"
                );
                return;
            }
        }
    }

    if matches!(event.kind, WatchEventKind::Removed) {
        remove_indexed_file(state, &event.path, "文件已删除，清理旧索引").await;
        return;
    }

    if matches!(event.kind, WatchEventKind::Renamed)
        && let Some(old_path) = event.old_path.as_ref()
        && old_path != &event.path
    {
        remove_indexed_file(state, old_path, "文件已重命名，清理旧路径索引").await;
    }

    if !is_supported_index_file(&event.path) {
        debug!(path = %event.path.display(), kind = ?event.kind, "目标路径不是受支持文本文件，跳过重建索引");
        set_runtime_idle(state, None).await;
        return;
    }

    {
        let mut runtime = state.indexing_runtime.write().await;
        runtime.phase = "scanning".to_string();
        runtime.last_scan_at = Some(unix_now_secs());
    }

    let metadata = match tokio::fs::metadata(&event.path).await {
        Ok(meta) => meta,
        Err(err) => {
            warn!(
                path = %event.path.display(),
                error = %err,
                "读取文件元数据失败，跳过本次索引"
            );
            let mut runtime = state.indexing_runtime.write().await;
            runtime.last_error = Some(err.to_string());
            runtime.phase = "idle".to_string();
            return;
        }
    };
    let file_size = i64::try_from(metadata.len()).unwrap_or(i64::MAX);
    let mtime_secs = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .unwrap_or(0);
    if let Err(err) = state
        .vector_store
        .upsert_catalog_entry(&event.path, watch_root, file_size, mtime_secs)
        .await
    {
        warn!(
            path = %event.path.display(),
            error = %err,
            "更新文件目录索引失败，已跳过本次事件"
        );
        set_runtime_idle(state, Some(err.to_string())).await;
        return;
    }
    let previous_state = state
        .vector_store
        .get_file_index_state(&event.path)
        .await
        .ok()
        .flatten();
    if let Some(prev) = previous_state.as_ref()
        && prev.file_size == file_size
        && prev.mtime_secs == mtime_secs
    {
        debug!(path = %event.path.display(), "文件元数据未变化，跳过重建索引");
        set_runtime_idle(state, None).await;
        return;
    }

    let raw_text = match tokio::fs::read_to_string(&event.path).await {
        Ok(text) => text,
        Err(err) => {
            warn!(
                path = %event.path.display(),
                error = %err,
                "文件读取失败（可能被占用），已跳过"
            );
            let mut runtime = state.indexing_runtime.write().await;
            runtime.last_error = Some(err.to_string());
            runtime.phase = "idle".to_string();
            return;
        }
    };
    let file_hash = hash_text(&raw_text);
    if let Some(prev) = previous_state
        && prev.content_hash == file_hash
    {
        debug!(path = %event.path.display(), "文件内容哈希未变化，跳过重建索引");
        if let Err(err) = state
            .vector_store
            .upsert_file_index_state(&event.path, file_size, mtime_secs, &file_hash)
            .await
        {
            warn!(
                path = %event.path.display(),
                error = %err,
                "刷新文件索引元数据失败"
            );
        }
        set_runtime_idle(state, None).await;
        return;
    }

    if let Err(err) = state
        .vector_store
        .mark_file_index_pending(&event.path, file_size, mtime_secs, &file_hash)
        .await
    {
        warn!(
            path = %event.path.display(),
            error = %err,
            "写入文件待索引状态失败，继续执行本次索引"
        );
    }

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
            let _ = state
                .vector_store
                .mark_file_index_failed(
                    &event.path,
                    file_size,
                    mtime_secs,
                    &file_hash,
                    &err.to_string(),
                )
                .await;
            let mut runtime = state.indexing_runtime.write().await;
            runtime.last_error = Some(err.to_string());
            runtime.phase = "idle".to_string();
            return;
        }
    };

    if chunks.is_empty() {
        debug!(path = %event.path.display(), "解析结果为空，清理旧索引并保留 catalog 记录");
        if let Err(err) = state.vector_store.purge_file_path(&event.path).await {
            warn!(
                path = %event.path.display(),
                error = %err,
                "清理空文档旧索引失败"
            );
        }
        let _ = state
            .vector_store
            .upsert_catalog_entry(&event.path, watch_root, file_size, mtime_secs)
            .await;
        if let Err(err) = state
            .vector_store
            .upsert_file_index_state(&event.path, file_size, mtime_secs, &file_hash)
            .await
        {
            warn!(
                path = %event.path.display(),
                error = %err,
                "写入空文档索引状态失败"
            );
        }
        set_runtime_idle(state, None).await;
        return;
    }

    {
        let mut runtime = state.indexing_runtime.write().await;
        runtime.phase = "embedding".to_string();
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
                let _ = state
                    .vector_store
                    .mark_file_index_failed(
                        &event.path,
                        file_size,
                        mtime_secs,
                        &file_hash,
                        &err.to_string(),
                    )
                    .await;
                let mut runtime = state.indexing_runtime.write().await;
                runtime.last_error = Some(err.to_string());
                runtime.phase = "idle".to_string();
                return;
            }
        }
    }

    if let Err(err) = state
        .vector_store
        .replace_document_index(
            &event.path,
            watch_root,
            mtime_secs,
            &file_hash,
            chunks.clone(),
            embeddings,
        )
        .await
    {
        error!(
            path = %event.path.display(),
            error = %err,
            "向量落盘失败，本次事件已跳过但守护进程继续运行"
        );
        let _ = state
            .vector_store
            .mark_file_index_failed(
                &event.path,
                file_size,
                mtime_secs,
                &file_hash,
                &err.to_string(),
            )
            .await;
        let mut runtime = state.indexing_runtime.write().await;
        runtime.last_error = Some(err.to_string());
        runtime.phase = "idle".to_string();
        return;
    }

    for chunk in chunks {
        let chunk_id = match state
            .vector_store
            .resolve_chunk_id(&chunk.file_path, chunk.chunk_index)
            .await
        {
            Ok(Some(id)) => id,
            Ok(None) => continue,
            Err(err) => {
                warn!(
                    path = %chunk.file_path.display(),
                    chunk_index = chunk.chunk_index,
                    error = %err,
                    "无法解析 chunk_id，跳过图谱任务入队"
                );
                continue;
            }
        };
        let chunk_hash = hash_text(&chunk.content);
        if let Err(err) = state
            .vector_store
            .enqueue_graph_task(chunk_id, &chunk_hash, &chunk.content)
            .await
        {
            warn!(
                path = %chunk.file_path.display(),
                chunk_index = chunk.chunk_index,
                error = %err,
                "图谱任务入队失败，后续可重试"
            );
        }
    }

    if let Some(tx) = graph_notify_tx {
        let _ = tx.send(()).await;
    }

    set_runtime_idle(state, None).await;
}

async fn remove_indexed_file(state: &Arc<AppState>, file_path: &std::path::Path, reason: &str) {
    {
        let mut runtime = state.indexing_runtime.write().await;
        runtime.phase = "scanning".to_string();
        runtime.last_scan_at = Some(unix_now_secs());
    }

    let purge_result = if is_likely_directory_path(file_path) {
        state.vector_store.purge_directory_path(file_path).await
    } else {
        state.vector_store.purge_file_path(file_path).await
    };

    match purge_result {
        Ok(true) => {
            info!(path = %file_path.display(), reason = reason, "文件索引清理完成");
            set_runtime_idle(state, None).await;
        }
        Ok(false) => {
            debug!(path = %file_path.display(), reason = reason, "文件不存在可清理索引，跳过");
            set_runtime_idle(state, None).await;
        }
        Err(err) => {
            warn!(
                path = %file_path.display(),
                reason = reason,
                error = %err,
                "清理旧文件索引失败"
            );
            set_runtime_idle(state, Some(err.to_string())).await;
        }
    }
}

async fn run_graph_worker(
    state: Arc<AppState>,
    mut notify_rx: mpsc::Receiver<()>,
) -> Result<(), EngineError> {
    info!("memori-core graph worker started");
    let mut channel_closed = false;

    loop {
        let runtime = state.indexing_runtime.read().await.clone();
        if runtime.paused
            || runtime.config.mode == IndexingMode::Manual
            || !is_within_schedule_window(&runtime.config)
        {
            sleep(Duration::from_millis(500)).await;
            if channel_closed && state.vector_store.count_graph_backlog().await.unwrap_or(0) == 0 {
                break;
            }
            continue;
        }

        match state.vector_store.fetch_next_graph_task().await? {
            Some(task) => {
                {
                    let mut runtime = state.indexing_runtime.write().await;
                    runtime.phase = "graphing".to_string();
                }
                let graph_data = match extract_entities(&task.content).await {
                    Ok(data) => data,
                    Err(err) => {
                        warn!(
                            chunk_id = task.chunk_id,
                            retry = task.retry_count + 1,
                            error = %err,
                            "图谱抽取失败，任务将重试"
                        );
                        state
                            .vector_store
                            .mark_graph_task_failed(task.task_id, task.retry_count + 1)
                            .await?;
                        let mut runtime = state.indexing_runtime.write().await;
                        runtime.last_error = Some(err.to_string());
                        continue;
                    }
                };

                if let Err(err) = state
                    .vector_store
                    .insert_graph(task.chunk_id, graph_data.nodes, graph_data.edges)
                    .await
                {
                    warn!(
                        chunk_id = task.chunk_id,
                        retry = task.retry_count + 1,
                        error = %err,
                        "图谱落盘失败，任务将重试"
                    );
                    state
                        .vector_store
                        .mark_graph_task_failed(task.task_id, task.retry_count + 1)
                        .await?;
                    let mut runtime = state.indexing_runtime.write().await;
                    runtime.last_error = Some(err.to_string());
                    continue;
                }

                state
                    .vector_store
                    .mark_graph_task_done(task.task_id)
                    .await?;
                let mut runtime = state.indexing_runtime.write().await;
                runtime.phase = "idle".to_string();
                runtime.last_error = None;
            }
            None => {
                if channel_closed {
                    if state.vector_store.count_graph_backlog().await.unwrap_or(0) == 0 {
                        break;
                    }
                } else {
                    match notify_rx.recv().await {
                        Some(_) => {}
                        None => channel_closed = true,
                    }
                }
                let cfg = state.indexing_runtime.read().await.config.clone();
                sleep(graph_worker_idle_delay(cfg.resource_budget)).await;
            }
        }
    }

    info!("memori-core graph worker exiting");
    Ok(())
}

fn graph_worker_idle_delay(budget: ResourceBudget) -> Duration {
    match budget {
        ResourceBudget::Low => Duration::from_millis(650),
        ResourceBudget::Balanced => Duration::from_millis(260),
        ResourceBudget::Fast => Duration::from_millis(80),
    }
}

fn is_within_schedule_window(config: &IndexingConfig) -> bool {
    if config.mode != IndexingMode::Scheduled {
        return true;
    }
    let Some(window) = config.schedule_window.as_ref() else {
        return true;
    };

    let Some(start_minutes) = parse_hhmm_to_minutes(&window.start) else {
        return true;
    };
    let Some(end_minutes) = parse_hhmm_to_minutes(&window.end) else {
        return true;
    };

    let now = unix_now_secs();
    let day_secs = 24 * 60 * 60;
    let minute_now = ((now.rem_euclid(day_secs)) / 60) as i32;

    if start_minutes <= end_minutes {
        minute_now >= start_minutes && minute_now <= end_minutes
    } else {
        minute_now >= start_minutes || minute_now <= end_minutes
    }
}

fn parse_hhmm_to_minutes(text: &str) -> Option<i32> {
    let mut parts = text.trim().split(':');
    let hour = parts.next()?.parse::<i32>().ok()?;
    let minute = parts.next()?.parse::<i32>().ok()?;
    if parts.next().is_some() || !(0..=23).contains(&hour) || !(0..=59).contains(&minute) {
        return None;
    }
    Some(hour * 60 + minute)
}

fn hash_text(text: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn unix_now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .unwrap_or(0)
}

async fn run_full_rebuild(
    state: &Arc<AppState>,
    root: &std::path::Path,
    graph_notify_tx: Option<&mpsc::Sender<()>>,
    reason: &str,
) -> Result<(), EngineError> {
    let previous_paused = {
        let mut runtime = state.indexing_runtime.write().await;
        let paused = runtime.paused;
        runtime.paused = true;
        runtime.phase = "scanning".to_string();
        runtime.last_scan_at = Some(unix_now_secs());
        runtime.last_error = None;
        paused
    };

    let rebuild_result = async {
        state.vector_store.begin_full_rebuild(reason).await?;
        state.vector_store.purge_all_index_data().await?;

        let existing_files = collect_supported_text_files_recursively(root.to_path_buf()).await;
        info!(
            root = %root.display(),
            reason = reason,
            file_count = existing_files.len(),
            "开始执行全量重建"
        );

        for path in existing_files {
            let event = WatchEvent {
                kind: WatchEventKind::Modified,
                path,
                old_path: None,
                observed_at: SystemTime::now(),
            };
            process_file_event(state, &event, graph_notify_tx, Some(root), true).await;

            let runtime = state.indexing_runtime.read().await.clone();
            if let Some(message) = runtime.last_error.filter(|msg| !msg.trim().is_empty()) {
                return Err(EngineError::IndexUnavailable {
                    reason: Some(format!("rebuild_file_failed:{message}")),
                });
            }
        }

        state.vector_store.finish_full_rebuild().await?;
        Ok::<(), EngineError>(())
    }
    .await;

    {
        let mut runtime = state.indexing_runtime.write().await;
        runtime.paused = previous_paused;
        runtime.last_scan_at = Some(unix_now_secs());
    }

    match rebuild_result {
        Ok(()) => {
            set_runtime_idle(state, None).await;
            Ok(())
        }
        Err(err) => {
            let failure_reason = format!("rebuild_failed:{err}");
            if let Err(mark_err) = state
                .vector_store
                .mark_rebuild_required(failure_reason.clone())
                .await
            {
                warn!(
                    error = %mark_err,
                    "全量重建失败后写回 required 状态也失败"
                );
            }
            set_runtime_idle(state, Some(err.to_string())).await;
            Err(err)
        }
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

#[cfg(test)]
mod tests {
    use super::{
        AppState, AskStatus, EgressMode, EngineError, EnterpriseModelPolicy, MemoriEngine,
        ModelProvider, QueryIntent, RuntimeModelConfig, WatchEvent, WatchEventKind, analyze_query,
        is_implementation_lookup, merge_document_candidates, process_file_event,
        should_refuse_for_insufficient_evidence, validate_runtime_model_settings,
    };
    use memori_parser::DocumentChunk;
    use memori_storage::RebuildState;
    use memori_storage::VectorStore;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("memori_vault_core_{name}_{unique}.db"))
    }

    async fn seed_indexed_file(state: &Arc<AppState>, file_path: &PathBuf) {
        state
            .vector_store
            .insert_chunks(
                vec![DocumentChunk {
                    file_path: file_path.clone(),
                    content: "seed content".to_string(),
                    chunk_index: 0,
                    heading_path: Vec::new(),
                    block_kind: memori_parser::ChunkBlockKind::Paragraph,
                }],
                vec![vec![0.1_f32, 0.2_f32]],
            )
            .await
            .expect("insert seed chunks");
        state
            .vector_store
            .upsert_file_index_state(file_path, 12, 34, "seed_hash")
            .await
            .expect("upsert seed index state");
    }

    #[tokio::test]
    async fn removed_event_purges_existing_index() {
        let db_path = temp_db_path("removed");
        let state = Arc::new(AppState::new(&db_path).expect("create app state"));
        let file_path = PathBuf::from("notes/removed.md");
        seed_indexed_file(&state, &file_path).await;

        let event = WatchEvent {
            kind: WatchEventKind::Removed,
            path: file_path.clone(),
            old_path: None,
            observed_at: SystemTime::now(),
        };

        process_file_event(&state, &event, None, None, false).await;

        assert!(
            state
                .vector_store
                .resolve_chunk_id(&file_path, 0)
                .await
                .expect("resolve chunk after remove")
                .is_none()
        );
        assert!(
            state
                .vector_store
                .get_file_index_state(&file_path)
                .await
                .expect("get file index after remove")
                .is_none()
        );

        drop(state);
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn local_only_blocks_remote_runtime() {
        let policy = EnterpriseModelPolicy {
            egress_mode: EgressMode::LocalOnly,
            allowed_model_endpoints: Vec::new(),
            allowed_models: Vec::new(),
        };
        let runtime = RuntimeModelConfig {
            provider: ModelProvider::OpenAiCompatible,
            endpoint: "https://api.openai.com/v1".to_string(),
            api_key: Some("secret".to_string()),
            chat_model: "gpt-4o-mini".to_string(),
            graph_model: "gpt-4o-mini".to_string(),
            embed_model: "text-embedding-3-small".to_string(),
        };

        let violation = validate_runtime_model_settings(&policy, &runtime)
            .expect_err("remote runtime should be blocked");
        assert_eq!(violation.code, "runtime_blocked_by_policy");
    }

    #[test]
    fn allowlist_requires_endpoint_and_models() {
        let policy = EnterpriseModelPolicy {
            egress_mode: EgressMode::Allowlist,
            allowed_model_endpoints: vec!["https://models.company.local/v1/".to_string()],
            allowed_models: vec!["approved-chat".to_string(), "approved-embed".to_string()],
        };
        let runtime = RuntimeModelConfig {
            provider: ModelProvider::OpenAiCompatible,
            endpoint: "https://models.company.local/v1".to_string(),
            api_key: None,
            chat_model: "approved-chat".to_string(),
            graph_model: "approved-chat".to_string(),
            embed_model: "denied-embed".to_string(),
        };

        let violation = validate_runtime_model_settings(&policy, &runtime)
            .expect_err("non-allowlisted model should be blocked");
        assert_eq!(violation.code, "model_not_allowlisted");
    }

    #[tokio::test]
    async fn renamed_event_to_unsupported_extension_only_purges_old_index() {
        let db_path = temp_db_path("rename_unsupported");
        let state = Arc::new(AppState::new(&db_path).expect("create app state"));
        let old_path = PathBuf::from("notes/rename_me.md");
        let new_path = PathBuf::from("notes/rename_me.pdf");
        seed_indexed_file(&state, &old_path).await;

        let event = WatchEvent {
            kind: WatchEventKind::Renamed,
            path: new_path.clone(),
            old_path: Some(old_path.clone()),
            observed_at: SystemTime::now(),
        };

        process_file_event(&state, &event, None, None, false).await;

        assert!(
            state
                .vector_store
                .resolve_chunk_id(&old_path, 0)
                .await
                .expect("resolve old chunk after rename")
                .is_none()
        );
        assert!(
            state
                .vector_store
                .get_file_index_state(&old_path)
                .await
                .expect("get old file index after rename")
                .is_none()
        );
        assert!(
            state
                .vector_store
                .get_file_index_state(&new_path)
                .await
                .expect("get new file index after rename")
                .is_none()
        );

        drop(state);
        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn removed_directory_event_purges_nested_indexes() {
        let db_path = temp_db_path("removed_dir");
        let state = Arc::new(AppState::new(&db_path).expect("create app state"));
        let nested_a = PathBuf::from("notes/project/a.md");
        let nested_b = PathBuf::from("notes/project/sub/b.txt");
        let outside = PathBuf::from("notes/other/c.md");

        seed_indexed_file(&state, &nested_a).await;
        seed_indexed_file(&state, &nested_b).await;
        seed_indexed_file(&state, &outside).await;

        let event = WatchEvent {
            kind: WatchEventKind::Removed,
            path: PathBuf::from("notes/project"),
            old_path: None,
            observed_at: SystemTime::now(),
        };

        process_file_event(&state, &event, None, None, false).await;

        assert!(
            state
                .vector_store
                .resolve_chunk_id(&nested_a, 0)
                .await
                .expect("resolve nested a")
                .is_none()
        );
        assert!(
            state
                .vector_store
                .resolve_chunk_id(&nested_b, 0)
                .await
                .expect("resolve nested b")
                .is_none()
        );
        assert!(
            state
                .vector_store
                .resolve_chunk_id(&outside, 0)
                .await
                .expect("resolve outside")
                .is_some()
        );

        drop(state);
        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn unchanged_metadata_branch_restores_idle_phase() {
        let db_path = temp_db_path("unchanged_meta");
        let state = Arc::new(AppState::new(&db_path).expect("create app state"));
        let file_path = PathBuf::from("notes/unchanged.md");

        let parent = std::env::temp_dir().join(format!(
            "memori_vault_core_file_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("duration since epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&parent).expect("create temp dir");
        let real_file_path = parent.join("unchanged.md");
        std::fs::write(&real_file_path, "same content").expect("write temp file");

        let metadata = std::fs::metadata(&real_file_path).expect("read metadata");
        let file_size = i64::try_from(metadata.len()).expect("file size fits i64");
        let mtime_secs = metadata
            .modified()
            .expect("modified time")
            .duration_since(UNIX_EPOCH)
            .expect("duration since epoch")
            .as_secs() as i64;
        state
            .vector_store
            .upsert_file_index_state(&real_file_path, file_size, mtime_secs, "seed_hash")
            .await
            .expect("seed file index state");

        let event = WatchEvent {
            kind: WatchEventKind::Modified,
            path: real_file_path.clone(),
            old_path: Some(file_path),
            observed_at: SystemTime::now(),
        };

        process_file_event(&state, &event, None, None, false).await;

        let runtime = state.indexing_runtime.read().await.clone();
        assert_eq!(runtime.phase, "idle");

        drop(state);
        let _ = std::fs::remove_file(&real_file_path);
        let _ = std::fs::remove_dir_all(&parent);
        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn search_is_blocked_when_index_rebuild_is_required() {
        let db_path = temp_db_path("search_blocked_required");
        let state = Arc::new(AppState::new(&db_path).expect("create app state"));
        state
            .vector_store
            .mark_rebuild_required("parser_format_changed")
            .await
            .expect("mark rebuild required");

        let (_tx, rx) = tokio::sync::mpsc::channel(8);
        let engine = MemoriEngine::new(state, rx);
        let err = engine
            .search("test query", 5, None)
            .await
            .expect_err("search should be blocked");

        assert!(matches!(err, EngineError::IndexUnavailable { .. }));

        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn retrieve_structured_empty_query_returns_insufficient_evidence() {
        let db_path = temp_db_path("retrieve_empty");
        let state = Arc::new(AppState::new(&db_path).expect("create app state"));
        let (_tx, rx) = tokio::sync::mpsc::channel(8);
        let engine = MemoriEngine::new(state, rx);

        let response = engine
            .retrieve_structured("   ", None, None)
            .await
            .expect("retrieve structured");

        assert_eq!(response.status, AskStatus::InsufficientEvidence);
        assert!(response.citations.is_empty());
        assert!(response.evidence.is_empty());

        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn daemon_rebuilds_required_index_and_returns_ready() {
        let db_path = temp_db_path("daemon_rebuild_required");
        let state = Arc::new(AppState::new(&db_path).expect("create app state"));
        state
            .vector_store
            .mark_rebuild_required("parser_format_changed")
            .await
            .expect("mark rebuild required");

        let temp_root = std::env::temp_dir().join(format!(
            "memori_vault_core_rebuild_root_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("duration since epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_root).expect("create temp root");

        let (tx, rx) = tokio::sync::mpsc::channel(8);
        drop(tx);

        let mut engine = MemoriEngine::new(state.clone(), rx);
        engine.watch_root = Some(temp_root.clone());
        engine.start_daemon().expect("start daemon");
        engine.shutdown().await.expect("shutdown daemon");

        let metadata = state
            .vector_store
            .read_index_metadata()
            .await
            .expect("read index metadata");
        assert_eq!(metadata.rebuild_state, RebuildState::Ready);
        assert!(metadata.rebuild_reason.is_none());

        drop(state);
        let _ = std::fs::remove_dir_all(&temp_root);
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn analyze_query_keeps_cjk_phrase_and_identifier_terms() {
        let analysis = analyze_query("长跳转公式是什么 POST /api/ask 周报8 week8_report.md");
        assert!(analysis.chunk_terms.iter().any(|term| term == "长跳转公式"));
        assert!(!analysis.chunk_terms.iter().any(|term| term == "是什么"));
        assert!(
            analysis
                .identifier_terms
                .iter()
                .any(|term| term == "week8_report.md")
        );
        assert!(analysis.identifier_terms.iter().any(|term| term == "api"));
        assert!(
            analysis
                .filename_like_terms
                .iter()
                .any(|term| term == "周报8")
        );
        assert!(analysis.flags.has_cjk);
        assert!(analysis.flags.has_path_like_token);
        assert!(analysis.flags.is_lookup_like);
    }

    #[test]
    fn classify_query_intent_marks_external_and_secret_queries() {
        assert_eq!(
            analyze_query("CEO of OpenAI").query_intent,
            QueryIntent::ExternalFact
        );
        assert_eq!(
            analyze_query("Bitcoin price today").query_intent,
            QueryIntent::ExternalFact
        );
        assert_eq!(
            analyze_query("hidden remote API key").query_intent,
            QueryIntent::SecretRequest
        );
    }

    #[test]
    fn document_merge_prefers_filename_signal_when_scores_are_stronger() {
        let analysis = analyze_query("week8_report.md");
        let merged = merge_document_candidates(
            &analysis,
            vec![memori_storage::DocumentSignalMatch {
                file_path: "docs/week8_report.md".to_string(),
                relative_path: "docs/week8_report.md".to_string(),
                file_name: "week8_report.md".to_string(),
                matched_fields: vec!["file_name".to_string()],
                score: 120,
            }],
            Vec::new(),
            vec![memori_storage::FtsDocumentMatch {
                doc_id: 1,
                file_path: "docs/other.md".to_string(),
                relative_path: "docs/other.md".to_string(),
                file_name: "other.md".to_string(),
                score: 0.4,
                heading_catalog_text: String::new(),
                document_search_text: String::new(),
            }],
        );

        assert_eq!(merged[0].file_name, "week8_report.md");
        assert_eq!(merged[0].document_reason, "filename");
    }

    #[test]
    fn document_merge_prefers_exact_symbol_for_implementation_lookup() {
        let analysis = analyze_query("ask_vault_structured");
        let merged = merge_document_candidates(
            &analysis,
            vec![memori_storage::DocumentSignalMatch {
                file_path: "memori-desktop/src/lib.rs".to_string(),
                relative_path: "memori-desktop/src/lib.rs".to_string(),
                file_name: "lib.rs".to_string(),
                matched_fields: vec!["exact_symbol".to_string()],
                score: 160,
            }],
            Vec::new(),
            vec![memori_storage::FtsDocumentMatch {
                doc_id: 1,
                file_path: "README.md".to_string(),
                relative_path: "README.md".to_string(),
                file_name: "README.md".to_string(),
                score: 3.0,
                heading_catalog_text: String::new(),
                document_search_text: "ask vault structured".to_string(),
            }],
        );

        assert_eq!(merged[0].relative_path, "memori-desktop/src/lib.rs");
        assert_eq!(merged[0].document_reason, "exact_symbol");
    }

    #[test]
    fn implementation_lookup_query_classification_is_enabled_for_code_symbols() {
        assert!(is_implementation_lookup(&analyze_query("POST /api/ask")));
        assert!(is_implementation_lookup(&analyze_query(
            "ask_vault_structured"
        )));
        assert!(is_implementation_lookup(&analyze_query(
            "query_analysis_ms"
        )));
        assert!(is_implementation_lookup(&analyze_query("week8_report.md")));
        assert!(!is_implementation_lookup(&analyze_query(
            "settings Advanced tab"
        )));
    }

    #[test]
    fn lookup_query_with_filename_and_lexical_support_is_not_rejected() {
        let analysis = analyze_query("week8_report.md");
        let evidence = vec![
            super::MergedEvidence {
                chunk: DocumentChunk {
                    file_path: PathBuf::from("docs/week8_report.md"),
                    content: "第八周周报摘要".to_string(),
                    chunk_index: 0,
                    heading_path: vec!["周报".to_string()],
                    block_kind: memori_parser::ChunkBlockKind::Paragraph,
                },
                relative_path: "docs/week8_report.md".to_string(),
                document_reason: "filename".to_string(),
                document_rank: 1,
                document_raw_score: Some(1.0),
                document_has_exact_signal: false,
                document_has_filename_signal: true,
                document_has_strict_lexical: false,
                lexical_strict_rank: Some(1),
                lexical_broad_rank: None,
                lexical_raw_score: Some(1.0),
                dense_rank: None,
                dense_raw_score: None,
                final_score: 1.0,
            },
            super::MergedEvidence {
                chunk: DocumentChunk {
                    file_path: PathBuf::from("docs/week8_report.md"),
                    content: "更多周报内容".to_string(),
                    chunk_index: 1,
                    heading_path: vec!["周报".to_string()],
                    block_kind: memori_parser::ChunkBlockKind::Paragraph,
                },
                relative_path: "docs/week8_report.md".to_string(),
                document_reason: "filename".to_string(),
                document_rank: 1,
                document_raw_score: Some(1.0),
                document_has_exact_signal: false,
                document_has_filename_signal: true,
                document_has_strict_lexical: false,
                lexical_strict_rank: Some(2),
                lexical_broad_rank: None,
                lexical_raw_score: Some(0.9),
                dense_rank: None,
                dense_raw_score: None,
                final_score: 0.9,
            },
        ];

        assert!(!should_refuse_for_insufficient_evidence(
            &analysis, &evidence
        ));
    }

    #[test]
    fn dense_only_long_query_is_rejected() {
        let analysis = analyze_query("请总结 week8_report.md 里的长跳转公式和实现细节");
        let evidence = vec![super::MergedEvidence {
            chunk: DocumentChunk {
                file_path: PathBuf::from("docs/week8_report.md"),
                content: "长跳转公式的实现细节".to_string(),
                chunk_index: 0,
                heading_path: vec!["周报".to_string()],
                block_kind: memori_parser::ChunkBlockKind::Paragraph,
            },
            relative_path: "docs/week8_report.md".to_string(),
            document_reason: "lexical_broad".to_string(),
            document_rank: 1,
            document_raw_score: Some(0.2),
            document_has_exact_signal: false,
            document_has_filename_signal: false,
            document_has_strict_lexical: false,
            lexical_strict_rank: None,
            lexical_broad_rank: None,
            lexical_raw_score: None,
            dense_rank: Some(1),
            dense_raw_score: Some(0.91),
            final_score: 1.0,
        }];

        assert!(should_refuse_for_insufficient_evidence(
            &analysis, &evidence
        ));
    }

    #[test]
    fn missing_file_lookup_without_document_signal_is_rejected() {
        let analysis = analyze_query("请总结 week8_report.md 的内容");
        let evidence = vec![super::MergedEvidence {
            chunk: DocumentChunk {
                file_path: PathBuf::from("docs/other.md"),
                content: "无关内容".to_string(),
                chunk_index: 0,
                heading_path: vec!["其他".to_string()],
                block_kind: memori_parser::ChunkBlockKind::Paragraph,
            },
            relative_path: "docs/other.md".to_string(),
            document_reason: "lexical_broad".to_string(),
            document_rank: 1,
            document_raw_score: Some(0.1),
            document_has_exact_signal: false,
            document_has_filename_signal: false,
            document_has_strict_lexical: false,
            lexical_strict_rank: None,
            lexical_broad_rank: Some(1),
            lexical_raw_score: Some(0.2),
            dense_rank: None,
            dense_raw_score: None,
            final_score: 0.6,
        }];

        assert!(should_refuse_for_insufficient_evidence(
            &analysis, &evidence
        ));
    }
}
