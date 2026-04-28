mod engine;
mod graph_extractor;
mod indexing;
mod llm_generator;
mod query;
mod retrieval;
mod runtime;

pub mod logging;

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Instant;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{collections::HashMap, hash::Hash, hash::Hasher};

pub use graph_extractor::GraphData;
use graph_extractor::extract_entities;
use llm_generator::generate_answer as generate_llm_answer;
pub use memori_parser::DocumentChunk;
use memori_parser::{ParserStub, parse_and_chunk};
pub use memori_storage::{
    LifecycleAction, MemoryEventRecord, MemoryLayer, MemoryLifecycleLogRecord, MemoryRecord,
    MemoryScope, MemorySearchOptions, MemorySourceType, MemoryStatus, NewMemoryEvent,
    NewMemoryRecord, UpdateMemoryRecord,
};
use memori_storage::{RebuildState, SqliteStore, StorageError};
use memori_vault::{
    MemoriVaultConfig, MemoriVaultError, MemoriVaultHandle, WatchEvent, WatchEventKind,
    create_event_channel, is_supported_content_file, spawn_memori_vault,
};
use thiserror::Error;
use tokio::sync::{RwLock, mpsc};
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

pub const DEFAULT_MODEL_PROVIDER: &str = "llama_cpp_local";
pub const DEFAULT_MODEL_ENDPOINT_OPENAI: &str = "https://api.openai.com";
pub const DEFAULT_LOCAL_EMBED_MODEL: &str = "Qwen3-Embedding-4B";
pub const DEFAULT_CHAT_MODEL: &str = "qwen2.5:7b";
pub const DEFAULT_GRAPH_MODEL: &str = "qwen2.5:7b";

/// Memori-Vault 本地常驻三模型默认端点 (llama-server HIP)
pub const DEFAULT_CHAT_ENDPOINT: &str = "http://localhost:18001";
pub const DEFAULT_GRAPH_ENDPOINT: &str = "http://localhost:18002";
pub const DEFAULT_EMBED_ENDPOINT: &str = "http://localhost:18003";
pub const DEFAULT_CHAT_MODEL_QWEN3: &str = "qwen3-14b";
pub const DEFAULT_GRAPH_MODEL_QWEN3: &str = "qwen3-8b";
pub const DEFAULT_EMBED_MODEL_QWEN3: &str = "Qwen3-Embedding-4B";
const DEFAULT_DB_FILE_NAME: &str = ".memori.db";
pub const MEMORI_DB_PATH_ENV: &str = "MEMORI_DB_PATH";
pub const MEMORI_MODEL_PROVIDER_ENV: &str = "MEMORI_MODEL_PROVIDER";
pub const MEMORI_MODEL_ENDPOINT_ENV: &str = "MEMORI_MODEL_ENDPOINT";
pub const MEMORI_MODEL_API_KEY_ENV: &str = "MEMORI_MODEL_API_KEY";
pub const MEMORI_CHAT_MODEL_ENV: &str = "MEMORI_CHAT_MODEL";
pub const MEMORI_GRAPH_MODEL_ENV: &str = "MEMORI_GRAPH_MODEL";
pub const MEMORI_EMBED_MODEL_ENV: &str = "MEMORI_EMBED_MODEL";
pub const MEMORI_CHAT_ENDPOINT_ENV: &str = "MEMORI_CHAT_ENDPOINT";
pub const MEMORI_GRAPH_ENDPOINT_ENV: &str = "MEMORI_GRAPH_ENDPOINT";
pub const MEMORI_EMBED_ENDPOINT_ENV: &str = "MEMORI_EMBED_ENDPOINT";
const QUERY_EMBEDDING_CACHE_SIZE: usize = 256;
const QUERY_EMBEDDING_CACHE_TTL_SECS: i64 = 300;
const DEFAULT_DOC_TOP_K: usize = 12;
const DEFAULT_CHUNK_CANDIDATE_K: usize = 20;
const DEFAULT_FINAL_ANSWER_K: usize = 6;
const RRF_K: f64 = 60.0;

fn is_supported_index_file(path: &std::path::Path) -> bool {
    is_supported_content_file(path)
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
    pub total_docs: u64,
    pub total_chunks: u64,
    pub progress_percent: u32,
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SourceGroup {
    pub group_id: String,
    pub canonical_title: String,
    pub file_paths: Vec<String>,
    pub relative_paths: Vec<String>,
    pub citation_indices: Vec<usize>,
    pub evidence_count: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnswerSourceMix {
    DocumentOnly,
    DocumentPlusMemory,
    MemoryOnly,
    #[default]
    Insufficient,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    RecallMiss,
    RankMiss,
    GatingFalseNegative,
    GenerationRefusal,
    CitationMiss,
    #[default]
    None,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct ContextBudgetReport {
    pub token_budget: usize,
    pub used_by_documents: usize,
    pub used_by_memory: usize,
    pub used_by_graph: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MemoryEvidence {
    pub id: i64,
    pub layer: MemoryLayer,
    pub scope: MemoryScope,
    pub memory_type: String,
    pub title: String,
    pub content: String,
    pub source_type: MemorySourceType,
    pub source_ref: String,
    pub confidence: f64,
    pub status: MemoryStatus,
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
    pub top_doc_distinct_term_hits: usize,
    pub top_doc_term_coverage: f64,
    pub gating_decision_reason: String,
    pub docs_phrase_quality: String,
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
    pub answer_source_mix: AnswerSourceMix,
    pub memory_context: Vec<MemoryEvidence>,
    pub source_groups: Vec<SourceGroup>,
    pub failure_class: FailureClass,
    pub context_budget_report: ContextBudgetReport,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RetrievalInspection {
    pub status: AskStatus,
    pub question: String,
    pub scope_paths: Vec<String>,
    pub citations: Vec<CitationItem>,
    pub evidence: Vec<EvidenceItem>,
    pub metrics: RetrievalMetrics,
    pub answer_source_mix: AnswerSourceMix,
    pub memory_context: Vec<MemoryEvidence>,
    pub source_groups: Vec<SourceGroup>,
    pub failure_class: FailureClass,
    pub context_budget_report: ContextBudgetReport,
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
    has_docs_phrase_signal: bool,
    docs_phrase_quality: Option<PhraseQuality>,
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
    #[allow(dead_code)]
    document_has_docs_phrase_signal: bool,
    document_docs_phrase_quality: Option<PhraseQuality>,
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
enum PhraseQuality {
    Specific,
    Generic,
}

impl PhraseQuality {
    fn as_str(self) -> &'static str {
        match self {
            Self::Specific => "specific",
            Self::Generic => "generic",
        }
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QueryFamily {
    DocsExplanatory,
    DocsApiLookup,
    ImplementationLookup,
}

impl QueryFamily {
    fn as_str(self) -> &'static str {
        match self {
            Self::DocsExplanatory => "docs_explanatory",
            Self::DocsApiLookup => "docs_api_lookup",
            Self::ImplementationLookup => "implementation_lookup",
        }
    }
}

#[derive(Debug, Clone)]
struct QueryAnalysis {
    normalized_query: String,
    lexical_query: String,
    document_routing_terms: Vec<String>,
    docs_phrase_terms: Vec<String>,
    chunk_terms: Vec<String>,
    support_terms: Vec<String>,
    filename_like_terms: Vec<String>,
    identifier_terms: Vec<String>,
    query_intent: QueryIntent,
    query_family: QueryFamily,
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
/// 当前持有 parser 占位、SQLite 持久化存储与本地 llama.cpp 客户端。
#[derive(Debug)]
pub struct AppState {
    pub parser: ParserStub,
    pub vector_store: Arc<SqliteStore>,
    pub embedding_client: LocalEmbeddingClient,
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
            embedding_client: LocalEmbeddingClient::default(),
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
    LlamaCppLocal,
    OpenAiCompatible,
}

impl ModelProvider {
    pub fn from_value(text: &str) -> Self {
        text.parse().unwrap_or(Self::LlamaCppLocal)
    }
}

impl FromStr for ModelProvider {
    type Err = &'static str;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text.trim().to_ascii_lowercase().as_str() {
            "llama_cpp_local" | "llamacpp_local" | "llama.cpp_local" | "ollama_local" => {
                Ok(Self::LlamaCppLocal)
            }
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
    pub chat_endpoint: String,
    pub chat_model: String,
    pub graph_endpoint: String,
    pub graph_model: String,
    pub embed_endpoint: String,
    pub embed_model: String,
    pub api_key: Option<String>,
    pub chat_context_length: Option<u32>,
    pub graph_context_length: Option<u32>,
    pub embed_context_length: Option<u32>,
    pub chat_concurrency: Option<u32>,
    pub graph_concurrency: Option<u32>,
    pub embed_concurrency: Option<u32>,
}

pub fn resolve_runtime_model_config_from_env() -> RuntimeModelConfig {
    let provider = std::env::var(MEMORI_MODEL_PROVIDER_ENV)
        .map(|v| ModelProvider::from_value(&v))
        .unwrap_or(ModelProvider::LlamaCppLocal);
    // 优先使用独立端点环境变量，其次使用常驻默认。
    // 不再回退 MEMORI_MODEL_ENDPOINT_ENV：前端设置只有一个 endpoint 输入框，
    // 回退到 legacy 变量会把三个独立端点全部覆盖为同一个值。
    let chat_endpoint = std::env::var(MEMORI_CHAT_ENDPOINT_ENV)
        .unwrap_or_else(|_| DEFAULT_CHAT_ENDPOINT.to_string());
    let graph_endpoint = std::env::var(MEMORI_GRAPH_ENDPOINT_ENV)
        .unwrap_or_else(|_| DEFAULT_GRAPH_ENDPOINT.to_string());
    let embed_endpoint = std::env::var(MEMORI_EMBED_ENDPOINT_ENV)
        .unwrap_or_else(|_| DEFAULT_EMBED_ENDPOINT.to_string());

    let api_key = std::env::var(MEMORI_MODEL_API_KEY_ENV).ok().and_then(|v| {
        let trimmed = v.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });

    let chat_model = std::env::var(MEMORI_CHAT_MODEL_ENV)
        .unwrap_or_else(|_| DEFAULT_CHAT_MODEL_QWEN3.to_string());
    let graph_model = std::env::var(MEMORI_GRAPH_MODEL_ENV)
        .unwrap_or_else(|_| DEFAULT_GRAPH_MODEL_QWEN3.to_string());
    let embed_model = std::env::var(MEMORI_EMBED_MODEL_ENV)
        .unwrap_or_else(|_| DEFAULT_EMBED_MODEL_QWEN3.to_string());

    RuntimeModelConfig {
        provider,
        chat_endpoint,
        chat_model,
        graph_endpoint,
        graph_model,
        embed_endpoint,
        embed_model,
        api_key,
        chat_context_length: std::env::var("MEMORI_CHAT_CONTEXT_LENGTH")
            .ok()
            .and_then(|v| v.parse().ok()),
        graph_context_length: std::env::var("MEMORI_GRAPH_CONTEXT_LENGTH")
            .ok()
            .and_then(|v| v.parse().ok()),
        embed_context_length: std::env::var("MEMORI_EMBED_CONTEXT_LENGTH")
            .ok()
            .and_then(|v| v.parse().ok()),
        chat_concurrency: std::env::var("MEMORI_CHAT_CONCURRENCY")
            .ok()
            .and_then(|v| v.parse().ok()),
        graph_concurrency: std::env::var("MEMORI_GRAPH_CONCURRENCY")
            .ok()
            .and_then(|v| v.parse().ok()),
        embed_concurrency: std::env::var("MEMORI_EMBED_CONCURRENCY")
            .ok()
            .and_then(|v| v.parse().ok()),
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
    if provider == ModelProvider::LlamaCppLocal {
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
    for (name, endpoint) in [
        ("chat", &runtime.chat_endpoint),
        ("graph", &runtime.graph_endpoint),
        ("embed", &runtime.embed_endpoint),
    ] {
        if let Err(violation) = validate_provider_request(
            policy,
            runtime.provider,
            endpoint,
            &[
                runtime.chat_model.clone(),
                runtime.graph_model.clone(),
                runtime.embed_model.clone(),
            ],
        ) {
            return Err(match violation.code.as_str() {
                "policy_violation" => PolicyViolation {
                    code: "runtime_blocked_by_policy".to_string(),
                    message: format!(
                        "Runtime model configuration rejected for {name} endpoint before startup"
                    ),
                },
                _ => violation,
            });
        }
    }
    Ok(())
}

/// 统一 Embedding 客户端（兼容 llama-server / vLLM / OpenAI 的 /v1/embeddings）。
#[derive(Debug, Clone)]
pub struct LocalEmbeddingClient {
    http: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    model: String,
}

impl Default for LocalEmbeddingClient {
    fn default() -> Self {
        let runtime = resolve_runtime_model_config_from_env();
        Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            base_url: runtime.embed_endpoint,
            api_key: runtime.api_key,
            model: runtime.embed_model,
        }
    }
}

impl LocalEmbeddingClient {
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            base_url: base_url.into(),
            api_key: None,
            model: model.into(),
        }
    }

    pub fn model_name(&self) -> &str {
        &self.model
    }

    pub async fn embed_text(&self, prompt: &str) -> Result<Vec<f32>, LocalModelClientError> {
        let url = format!("{}/v1/embeddings", self.base_url.trim_end_matches('/'));
        let mut request = self.http.post(url).json(&OpenAiEmbeddingRequest {
            model: &self.model,
            input: prompt,
        });
        if let Some(key) = self.api_key.as_ref() {
            request = request.bearer_auth(key);
        }

        let response = request
            .send()
            .await
            .map_err(LocalModelClientError::Request)?;
        let status = response.status();
        if !status.is_success() {
            let body = match response.text().await {
                Ok(text) => text,
                Err(err) => format!("<读取响应体失败: {err}>"),
            };

            return Err(LocalModelClientError::HttpStatus {
                status: status.as_u16(),
                body,
            });
        }

        let parsed: OpenAiEmbeddingResponse = response
            .json()
            .await
            .map_err(LocalModelClientError::Request)?;

        let embedding = parsed
            .data
            .into_iter()
            .next()
            .map(|item| item.embedding)
            .unwrap_or_default();
        if embedding.is_empty() {
            return Err(LocalModelClientError::EmptyEmbedding);
        }
        Ok(embedding)
    }
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
pub enum LocalModelClientError {
    #[error("Embedding 请求失败: {0}")]
    Request(#[source] reqwest::Error),

    #[error("本地模型服务返回非成功状态: {status}, body: {body}")]
    HttpStatus { status: u16, body: String },

    #[error("本地模型服务返回空向量")]
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
    LocalModel(#[from] LocalModelClientError),

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

impl Clone for MemoriEngine {
    fn clone(&self) -> Self {
        Self {
            state: Arc::clone(&self.state),
            event_rx: None,
            daemon_task: None,
            graph_worker_task: None,
            memori_vault_handle: None,
            watch_root: self.watch_root.clone(),
            graph_notify_tx: self.graph_notify_tx.clone(),
        }
    }
}

#[cfg(test)]
pub(crate) use engine::{build_memory_context_for_prompt, should_allow_memory_only_answer};
pub(crate) use indexing::*;
pub(crate) use query::*;
pub use retrieval::build_query_terms_for_offline_embedding;
pub(crate) use retrieval::*;
pub use runtime::generate_answer_with_context;
pub(crate) use runtime::*;

#[cfg(test)]
mod tests {
    use super::{
        AppState, AskStatus, EgressMode, EngineError, EnterpriseModelPolicy, MemoriEngine,
        MemoryEvidence, MemoryLayer, MemoryScope, MemorySourceType, MemoryStatus, MergedEvidence,
        ModelProvider, QueryFamily, QueryIntent, RetrievalMetrics, RuntimeModelConfig, WatchEvent,
        WatchEventKind, analyze_query, apply_gating_metrics, build_citations,
        build_memory_context_for_prompt, document_signal_query, has_strong_document_signal,
        is_implementation_lookup, merge_document_candidates, process_file_event,
        should_allow_memory_only_answer, should_refuse_for_insufficient_evidence,
        validate_runtime_model_settings,
    };
    use memori_parser::DocumentChunk;
    use memori_storage::RebuildState;
    use memori_storage::VectorStore;
    use std::fs;
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
    fn build_citations_dedupes_identical_rendered_excerpt_from_same_file() {
        let file_path = std::env::temp_dir().join(format!(
            "memori_citation_excerpt_{}.md",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time before unix epoch")
                .as_nanos()
        ));
        let content = "Project note: query_analysis_ms is emitted in retrieval metrics, and query_analysis_ms should stay visible to users for debugging.\n\nSecond paragraph stays separate.";
        fs::write(&file_path, content).expect("write temp markdown");

        let evidence = vec![
            MergedEvidence {
                chunk: DocumentChunk {
                    file_path: file_path.clone(),
                    content: "query_analysis_ms is emitted in retrieval metrics".to_string(),
                    chunk_index: 0,
                    heading_path: vec!["Metrics".to_string()],
                    block_kind: memori_parser::ChunkBlockKind::Paragraph,
                },
                relative_path: "notes/metrics.md".to_string(),
                document_reason: "lexical_strict".to_string(),
                document_rank: 1,
                document_raw_score: Some(1.0),
                document_has_exact_signal: false,
                document_has_docs_phrase_signal: false,
                document_docs_phrase_quality: None,
                document_has_filename_signal: false,
                document_has_strict_lexical: true,
                lexical_strict_rank: Some(1),
                lexical_broad_rank: None,
                lexical_raw_score: Some(1.0),
                dense_rank: None,
                dense_raw_score: None,
                final_score: 1.0,
            },
            MergedEvidence {
                chunk: DocumentChunk {
                    file_path: file_path.clone(),
                    content: "query_analysis_ms should stay visible to users".to_string(),
                    chunk_index: 1,
                    heading_path: vec!["Metrics".to_string()],
                    block_kind: memori_parser::ChunkBlockKind::Paragraph,
                },
                relative_path: "notes/metrics.md".to_string(),
                document_reason: "mixed".to_string(),
                document_rank: 1,
                document_raw_score: Some(0.9),
                document_has_exact_signal: false,
                document_has_docs_phrase_signal: false,
                document_docs_phrase_quality: None,
                document_has_filename_signal: false,
                document_has_strict_lexical: true,
                lexical_strict_rank: Some(2),
                lexical_broad_rank: None,
                lexical_raw_score: Some(0.9),
                dense_rank: Some(1),
                dense_raw_score: Some(0.8),
                final_score: 0.95,
            },
        ];

        let citations = build_citations(&evidence);

        assert_eq!(citations.len(), 1);
        assert_eq!(citations[0].index, 1);
        assert_eq!(citations[0].relative_path, "notes/metrics.md");

        let _ = fs::remove_file(file_path);
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
            chat_endpoint: "https://api.openai.com/v1".to_string(),
            chat_model: "gpt-4o-mini".to_string(),
            graph_endpoint: "https://api.openai.com/v1".to_string(),
            graph_model: "gpt-4o-mini".to_string(),
            embed_endpoint: "https://api.openai.com/v1".to_string(),
            embed_model: "text-embedding-3-small".to_string(),
            api_key: Some("secret".to_string()),
            chat_context_length: None,
            graph_context_length: None,
            embed_context_length: None,
            chat_concurrency: None,
            graph_concurrency: None,
            embed_concurrency: None,
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
            chat_endpoint: "https://models.company.local/v1".to_string(),
            chat_model: "approved-chat".to_string(),
            graph_endpoint: "https://models.company.local/v1".to_string(),
            graph_model: "approved-chat".to_string(),
            embed_endpoint: "https://models.company.local/v1".to_string(),
            embed_model: "denied-embed".to_string(),
            api_key: None,
            chat_context_length: None,
            graph_context_length: None,
            embed_context_length: None,
            chat_concurrency: None,
            graph_concurrency: None,
            embed_concurrency: None,
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
    fn analyze_query_extracts_generic_cjk_backoff_terms() {
        let founded = analyze_query("北极星生物计算成立于");
        assert!(
            founded
                .chunk_terms
                .iter()
                .any(|term| term == "北极星生物计算")
        );

        let description = analyze_query("星海系统是做什么的");
        assert!(
            description
                .chunk_terms
                .iter()
                .any(|term| term == "星海系统")
        );

        let short_entity = analyze_query("腾讯成立于");
        assert!(short_entity.chunk_terms.iter().any(|term| term == "腾讯"));
    }

    #[test]
    fn analyze_query_splits_mixed_script_entity_boundaries() {
        let analysis = analyze_query("北极星生物计算PolarisBioCompute成立于");
        assert!(
            analysis
                .chunk_terms
                .iter()
                .any(|term| term == "北极星生物计算")
        );
        assert!(
            analysis
                .chunk_terms
                .iter()
                .any(|term| term == "polarisbiocompute")
        );

        let reverse = analyze_query("PolarisBioCompute北极星生物计算");
        assert!(
            reverse
                .chunk_terms
                .iter()
                .any(|term| term == "北极星生物计算")
        );
        assert!(
            reverse
                .chunk_terms
                .iter()
                .any(|term| term == "polarisbiocompute")
        );
    }

    #[test]
    fn analyze_query_extracts_support_terms_for_descriptive_cjk_questions() {
        let analysis = analyze_query("新增的岗位是什么");
        assert!(analysis.support_terms.iter().any(|term| term == "新增"));
        assert!(analysis.support_terms.iter().any(|term| term == "岗位"));
        assert!(!analysis.support_terms.iter().any(|term| term == "是什么"));
        assert!(
            !analysis
                .support_terms
                .iter()
                .any(|term| term == "新增的岗位是什么")
        );
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
    fn memory_intent_only_allows_explicit_memory_questions() {
        let document_question = analyze_query("What does the onboarding document say?");
        let memory_question = analyze_query("What did I say about my project preference earlier?");
        let memory = MemoryEvidence {
            id: 1,
            layer: MemoryLayer::Ltm,
            scope: MemoryScope::Project,
            memory_type: "preference".to_string(),
            title: "Language".to_string(),
            content: "Prefer concise Chinese answers.".to_string(),
            source_type: MemorySourceType::ConversationTurn,
            source_ref: "conversation_turn:test".to_string(),
            confidence: 0.9,
            status: MemoryStatus::Active,
        };

        assert!(!should_allow_memory_only_answer(
            &document_question,
            std::slice::from_ref(&memory)
        ));
        assert!(should_allow_memory_only_answer(&memory_question, &[memory]));
    }

    #[test]
    fn memory_prompt_context_reports_token_budget() {
        let memory = MemoryEvidence {
            id: 7,
            layer: MemoryLayer::Mtm,
            scope: MemoryScope::Project,
            memory_type: "decision".to_string(),
            title: "Architecture decision".to_string(),
            content: "Graph stays evidence-only and does not affect main retrieval ranking."
                .to_string(),
            source_type: MemorySourceType::ToolEvent,
            source_ref: "tool_event:test".to_string(),
            confidence: 0.85,
            status: MemoryStatus::Active,
        };

        let (context, tokens) = build_memory_context_for_prompt(&[memory], 1_000);
        assert!(context.contains("Memory #7"));
        assert!(context.contains("source_ref: tool_event:test"));
        assert!(tokens > 0);
    }

    #[test]
    fn classify_query_family_distinguishes_docs_api_and_implementation_queries() {
        assert_eq!(
            analyze_query("POST /api/auth/oidc/login return?").query_family,
            QueryFamily::DocsApiLookup
        );
        assert_eq!(
            analyze_query("GET /api/admin/metrics").query_family,
            QueryFamily::DocsApiLookup
        );
        assert_eq!(
            analyze_query("ask_vault_structured 是哪个入口？").query_family,
            QueryFamily::ImplementationLookup
        );
        assert_eq!(
            analyze_query("How do you start server mode?").query_family,
            QueryFamily::DocsExplanatory
        );
    }

    #[test]
    fn document_signal_query_keeps_docs_terms_for_explanatory_queries() {
        let analysis = analyze_query("What does the tutorial say if vault stats stay at 0?");
        let query = document_signal_query(&analysis);
        assert!(!query.trim().is_empty());
        assert!(query.contains("tutorial"));
        assert!(query.contains("stats"));
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
                phrase_specific: false,
            }],
            Vec::new(),
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
                phrase_specific: false,
            }],
            Vec::new(),
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
    fn document_merge_prefers_docs_phrase_for_docs_api_lookup() {
        let analysis = analyze_query("POST /api/auth/oidc/login return?");
        let merged = merge_document_candidates(
            &analysis,
            Vec::new(),
            vec![memori_storage::DocumentSignalMatch {
                file_path: "docs/guides/enterprise.md".to_string(),
                relative_path: "docs/guides/enterprise.md".to_string(),
                file_name: "enterprise.md".to_string(),
                matched_fields: vec!["docs_phrase".to_string()],
                score: 180,
                phrase_specific: true,
            }],
            Vec::new(),
            vec![memori_storage::FtsDocumentMatch {
                doc_id: 1,
                file_path: "memori-server/src/main.rs".to_string(),
                relative_path: "memori-server/src/main.rs".to_string(),
                file_name: "main.rs".to_string(),
                score: 5.0,
                heading_catalog_text: String::new(),
                document_search_text: "POST /api/auth/oidc/login".to_string(),
            }],
        );

        assert_eq!(merged[0].relative_path, "docs/guides/enterprise.md");
        assert_eq!(merged[0].document_reason, "docs_phrase");
    }

    #[test]
    fn document_merge_demotes_generic_docs_phrase_for_docs_queries() {
        let analysis = analyze_query("岗位是什么");
        let merged = merge_document_candidates(
            &analysis,
            Vec::new(),
            vec![memori_storage::DocumentSignalMatch {
                file_path: "docs/overview.md".to_string(),
                relative_path: "docs/overview.md".to_string(),
                file_name: "overview.md".to_string(),
                matched_fields: vec!["docs_phrase".to_string()],
                score: 120,
                phrase_specific: false,
            }],
            vec![memori_storage::FtsDocumentMatch {
                doc_id: 1,
                file_path: "docs/hiring.md".to_string(),
                relative_path: "docs/hiring.md".to_string(),
                file_name: "hiring.md".to_string(),
                score: 1.2,
                heading_catalog_text: "招聘计划".to_string(),
                document_search_text: "新增 12 个岗位".to_string(),
            }],
            Vec::new(),
        );

        assert_eq!(merged[0].relative_path, "docs/hiring.md");
        assert_eq!(merged[0].document_reason, "lexical_strict");
    }

    #[test]
    fn generic_docs_phrase_is_not_treated_as_strong_signal() {
        let item = MergedEvidence {
            chunk: DocumentChunk {
                file_path: PathBuf::from("docs/overview.md"),
                content: "岗位概览".to_string(),
                chunk_index: 0,
                heading_path: vec!["概览".to_string()],
                block_kind: memori_parser::ChunkBlockKind::Paragraph,
            },
            relative_path: "docs/overview.md".to_string(),
            document_reason: "docs_phrase".to_string(),
            document_rank: 1,
            document_raw_score: Some(1.0),
            document_has_exact_signal: false,
            document_has_docs_phrase_signal: true,
            document_docs_phrase_quality: Some(super::PhraseQuality::Generic),
            document_has_filename_signal: false,
            document_has_strict_lexical: false,
            lexical_strict_rank: None,
            lexical_broad_rank: Some(1),
            lexical_raw_score: Some(0.8),
            dense_rank: None,
            dense_raw_score: None,
            final_score: 1.0,
        };

        assert!(!has_strong_document_signal(&item));
    }

    #[test]
    fn implementation_lookup_query_classification_is_enabled_for_code_symbols() {
        assert!(is_implementation_lookup(&analyze_query(
            "POST /api/ask 现在返回什么协议？"
        )));
        assert!(is_implementation_lookup(&analyze_query(
            "ask_vault_structured"
        )));
        assert!(is_implementation_lookup(&analyze_query(
            "query_analysis_ms"
        )));
        assert!(is_implementation_lookup(&analyze_query("week8_report.md")));
        assert!(!is_implementation_lookup(&analyze_query(
            "POST /api/auth/oidc/login return?"
        )));
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
                document_has_docs_phrase_signal: false,
                document_docs_phrase_quality: None,
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
                document_has_docs_phrase_signal: false,
                document_docs_phrase_quality: None,
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
            document_has_docs_phrase_signal: false,
            document_docs_phrase_quality: None,
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
    fn non_lookup_coverage_release_populates_metrics() {
        let analysis = analyze_query("新增的岗位是什么");
        let evidence = vec![super::MergedEvidence {
            chunk: DocumentChunk {
                file_path: PathBuf::from("docs/hiring.md"),
                content: "研发中心计划新增 12 个岗位，岗位包括后端与前端".to_string(),
                chunk_index: 0,
                heading_path: vec!["招聘计划".to_string()],
                block_kind: memori_parser::ChunkBlockKind::Paragraph,
            },
            relative_path: "docs/hiring.md".to_string(),
            document_reason: "lexical_broad".to_string(),
            document_rank: 1,
            document_raw_score: Some(0.8),
            document_has_exact_signal: false,
            document_has_docs_phrase_signal: false,
            document_docs_phrase_quality: None,
            document_has_filename_signal: false,
            document_has_strict_lexical: false,
            lexical_strict_rank: None,
            lexical_broad_rank: Some(1),
            lexical_raw_score: Some(0.6),
            dense_rank: None,
            dense_raw_score: None,
            final_score: 1.0,
        }];

        let mut metrics = RetrievalMetrics::default();
        let refused = apply_gating_metrics(&mut metrics, &analysis, &evidence);
        assert!(!refused);
        assert_eq!(metrics.gating_decision_reason, "coverage_release");
        assert!(metrics.top_doc_distinct_term_hits >= 2);
        assert!(metrics.top_doc_term_coverage >= 0.5);
    }

    #[test]
    fn lookup_like_high_coverage_lexical_evidence_is_not_rejected() {
        let analysis =
            analyze_query("物聯網 Internet of Things IoT UUID 通過網路傳輸數據的能力是什麼");
        assert!(analysis.flags.is_lookup_like);

        let evidence = vec![super::MergedEvidence {
            chunk: DocumentChunk {
                file_path: PathBuf::from("docs/iot.md"),
                content: "物聯網（英語：Internet of Things，簡稱 IoT）是一種計算設備、機械、數位機器相互關聯的系統，具備通用唯一辨識碼 UUID，並具有通過網路傳輸數據的能力。".to_string(),
                chunk_index: 0,
                heading_path: vec!["物联网".to_string()],
                block_kind: memori_parser::ChunkBlockKind::Paragraph,
            },
            relative_path: "docs/iot.md".to_string(),
            document_reason: "lexical_broad".to_string(),
            document_rank: 1,
            document_raw_score: Some(1.0),
            document_has_exact_signal: false,
            document_has_docs_phrase_signal: false,
            document_docs_phrase_quality: None,
            document_has_filename_signal: false,
            document_has_strict_lexical: false,
            lexical_strict_rank: None,
            lexical_broad_rank: Some(1),
            lexical_raw_score: Some(1.0),
            dense_rank: Some(1),
            dense_raw_score: Some(0.9),
            final_score: 1.0,
        }];

        let mut metrics = RetrievalMetrics::default();
        let refused = apply_gating_metrics(&mut metrics, &analysis, &evidence);
        assert!(!refused);
        assert_eq!(
            metrics.gating_decision_reason,
            "high_coverage_lexical_release"
        );
        assert!(metrics.top_doc_term_coverage >= 0.65);
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
            document_has_docs_phrase_signal: false,
            document_docs_phrase_quality: None,
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

    #[test]
    fn explicit_insufficient_context_answer_is_not_treated_as_success() {
        assert!(super::engine::answer_indicates_insufficient_evidence(
            "当前上下文不足，缺少关于本周学习内容的直接记录。"
        ));
        assert!(super::engine::answer_indicates_insufficient_evidence(
            "There is insufficient context to answer this reliably."
        ));
    }

    #[test]
    fn grounded_answer_text_is_not_misclassified_as_insufficient() {
        assert!(!super::engine::answer_indicates_insufficient_evidence(
            "本周主要学习了 FORTIFY 绕过、CANNARY 检查与若干改进事项。"
        ));
    }
}
