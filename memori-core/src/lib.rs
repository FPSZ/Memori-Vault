mod embedding_client;
mod error;
mod engine;
mod model_config;
mod engine_retrieve;
mod engine_search;
mod filter;
mod graph_extractor;
mod indexing;
mod indexing_graph;
mod indexing_rebuild;
mod llm_generator;
mod query;
mod query_utils;
mod retrieval;
mod retrieval_eval;
mod retrieval_output;
mod runtime;

pub mod logging;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{collections::HashMap, hash::Hash, hash::Hasher};

pub use embedding_client::LocalEmbeddingClient;
pub use error::*;
pub use filter::IndexFilterConfig;
pub use model_config::*;
pub use graph_extractor::GraphData;
use graph_extractor::extract_entities;
use llm_generator::generate_answer as generate_llm_answer;
pub use memori_parser::DocumentChunk;
use memori_parser::{ParserStub, parse_and_chunk};
pub use memori_storage::{
    GraphEdge, GraphNeighbors, GraphNode, LifecycleAction, MemoryEventRecord, MemoryLayer,
    MemoryLifecycleLogRecord, MemoryRecord, MemoryScope, MemorySearchOptions, MemorySourceType,
    MemoryStatus, NewMemoryEvent, NewMemoryRecord, UpdateMemoryRecord,
};
use memori_storage::{RebuildState, SqliteStore, StorageError};
use memori_vault::{
    MemoriVaultConfig, MemoriVaultHandle, WatchEvent, WatchEventKind,
    create_event_channel, is_supported_content_file, spawn_memori_vault,
};
use tokio::sync::{RwLock, mpsc};
use tokio::task::JoinHandle;
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
    if metadata.rebuild_state != RebuildState::Ready
        && metadata
            .rebuild_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("retryable_files_remaining"))
        && state.vector_store.count_chunks().await.unwrap_or(0) > 0
    {
        return Ok(());
    }
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
struct EvidenceRetrievalResult {
    candidate_docs: Vec<DocumentCandidate>,
    evidence: Vec<MergedEvidence>,
}

#[derive(Debug, Clone)]
struct CompoundEvidenceResult {
    evidence: Vec<MergedEvidence>,
    matched_parts: usize,
    partial: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompoundQueryPart {
    topic: String,
    query: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompoundQueryPlan {
    parts: Vec<CompoundQueryPart>,
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
    filter_config: Option<filter::IndexFilterConfig>,
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
                filter_config: None,
            })),
        })
    }
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

pub(crate) use engine::{build_memory_context_for_prompt, should_allow_memory_only_answer};
pub(crate) use indexing::*;
pub(crate) use indexing_graph::*;
pub(crate) use indexing_rebuild::*;
pub(crate) use query::*;
pub(crate) use query_utils::*;
pub use retrieval_output::build_query_terms_for_offline_embedding;
pub(crate) use retrieval::*;
pub(crate) use retrieval_eval::*;
pub(crate) use retrieval_output::*;
pub use runtime::generate_answer_with_context;
pub(crate) use runtime::*;


#[cfg(test)]
mod tests;
