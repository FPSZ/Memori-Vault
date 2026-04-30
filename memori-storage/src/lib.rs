use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use memori_parser::{ChunkBlockKind, DocumentChunk, PARSER_FORMAT_VERSION};
use rusqlite::{Connection, ErrorCode, OptionalExtension, params, params_from_iter};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::info;

pub const DB_SCHEMA_VERSION: u32 = 2;
pub const INDEX_FORMAT_VERSION: u32 = 4;

const METADATA_KEY_INDEX_FORMAT_VERSION: &str = "index_format_version";
const METADATA_KEY_PARSER_FORMAT_VERSION: &str = "parser_format_version";
const METADATA_KEY_REBUILD_STATE: &str = "rebuild_state";
const METADATA_KEY_REBUILD_REASON: &str = "rebuild_reason";
const METADATA_KEY_LAST_REBUILD_AT: &str = "last_rebuild_at";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RebuildState {
    Ready,
    Required,
    Rebuilding,
}

impl RebuildState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Required => "required",
            Self::Rebuilding => "rebuilding",
        }
    }

    fn from_stored(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "required" => Self::Required,
            "rebuilding" => Self::Rebuilding,
            _ => Self::Ready,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexMetadata {
    pub index_format_version: u32,
    pub parser_format_version: u32,
    pub rebuild_state: RebuildState,
    pub rebuild_reason: Option<String>,
    pub last_rebuild_at: Option<i64>,
}

/// 图谱节点。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub name: String,
    pub description: Option<String>,
}

/// 图谱边。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphEdge {
    pub id: String,
    pub source_node: String,
    pub target_node: String,
    pub relation: String,
}

/// 存储在内存中的单条向量记录。
#[derive(Debug, Clone)]
pub struct StoredVectorRecord {
    pub chunk: DocumentChunk,
    pub embedding: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileIndexState {
    pub file_path: String,
    pub file_size: i64,
    pub mtime_secs: i64,
    pub content_hash: String,
    pub indexed_at: i64,
    pub index_status: String,
    pub last_error: Option<String>,
    pub parser_format_version: u32,
    pub index_format_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatalogEntry {
    pub file_path: String,
    pub relative_path: String,
    pub parent_dir: String,
    pub file_name: String,
    pub file_ext: String,
    pub file_size: i64,
    pub mtime_secs: i64,
    pub discovered_at: i64,
    pub removed_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentRecord {
    pub id: i64,
    pub file_path: String,
    pub relative_path: String,
    pub file_name: String,
    pub file_ext: String,
    pub last_modified: i64,
    pub indexed_at: i64,
    pub chunk_count: u32,
    pub content_char_count: u32,
    pub heading_catalog_text: String,
    pub document_search_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChunkRecord {
    pub id: i64,
    pub doc_id: i64,
    pub chunk_index: usize,
    pub content: String,
    pub heading_path: Vec<String>,
    pub block_kind: String,
    pub char_len: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FtsChunkMatch {
    pub chunk_id: i64,
    pub doc_id: i64,
    pub file_path: String,
    pub relative_path: String,
    pub file_name: String,
    pub chunk_index: usize,
    pub score: f64,
    pub content: String,
    pub heading_path: Vec<String>,
    pub block_kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FtsDocumentMatch {
    pub doc_id: i64,
    pub file_path: String,
    pub relative_path: String,
    pub file_name: String,
    pub score: f64,
    pub heading_catalog_text: String,
    pub document_search_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentSignalMatch {
    pub file_path: String,
    pub relative_path: String,
    pub file_name: String,
    pub matched_fields: Vec<String>,
    pub score: i64,
    #[serde(default)]
    pub phrase_specific: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphTaskRecord {
    pub task_id: i64,
    pub chunk_id: i64,
    pub content: String,
    pub content_hash: String,
    pub status: String,
    pub retry_count: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct GraphNeighbors {
    pub center: Option<GraphNode>,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub source_chunks: Vec<ChunkRecord>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScope {
    User,
    #[default]
    Project,
    Session,
    Agent,
    Document,
}

impl MemoryScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Project => "project",
            Self::Session => "session",
            Self::Agent => "agent",
            Self::Document => "document",
        }
    }
}

impl std::str::FromStr for MemoryScope {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "user" => Ok(Self::User),
            "session" => Ok(Self::Session),
            "agent" => Ok(Self::Agent),
            "document" => Ok(Self::Document),
            "project" | "" => Ok(Self::Project),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryLayer {
    Stm,
    #[default]
    Mtm,
    Ltm,
    Graph,
    Policy,
}

impl MemoryLayer {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stm => "stm",
            Self::Mtm => "mtm",
            Self::Ltm => "ltm",
            Self::Graph => "graph",
            Self::Policy => "policy",
        }
    }
}

impl std::str::FromStr for MemoryLayer {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "stm" => Ok(Self::Stm),
            "ltm" => Ok(Self::Ltm),
            "graph" => Ok(Self::Graph),
            "policy" => Ok(Self::Policy),
            "mtm" | "" => Ok(Self::Mtm),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemorySourceType {
    DocumentChunk,
    #[default]
    ConversationTurn,
    ToolEvent,
    SystemEvent,
    MarkdownNote,
    GraphEdge,
}

impl MemorySourceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::DocumentChunk => "document_chunk",
            Self::ConversationTurn => "conversation_turn",
            Self::ToolEvent => "tool_event",
            Self::SystemEvent => "system_event",
            Self::MarkdownNote => "markdown_note",
            Self::GraphEdge => "graph_edge",
        }
    }
}

impl std::str::FromStr for MemorySourceType {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "document_chunk" => Ok(Self::DocumentChunk),
            "tool_event" => Ok(Self::ToolEvent),
            "system_event" => Ok(Self::SystemEvent),
            "markdown_note" => Ok(Self::MarkdownNote),
            "graph_edge" => Ok(Self::GraphEdge),
            "conversation_turn" | "" => Ok(Self::ConversationTurn),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    #[default]
    Active,
    Pending,
    Superseded,
    Deleted,
}

impl MemoryStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Pending => "pending",
            Self::Superseded => "superseded",
            Self::Deleted => "deleted",
        }
    }
}

impl std::str::FromStr for MemoryStatus {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "pending" => Ok(Self::Pending),
            "superseded" => Ok(Self::Superseded),
            "deleted" => Ok(Self::Deleted),
            "active" | "" => Ok(Self::Active),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleAction {
    #[default]
    Add,
    Update,
    Supersede,
    Delete,
    Noop,
}

impl LifecycleAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Update => "update",
            Self::Supersede => "supersede",
            Self::Delete => "delete",
            Self::Noop => "noop",
        }
    }
}

impl std::str::FromStr for LifecycleAction {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "update" => Ok(Self::Update),
            "supersede" => Ok(Self::Supersede),
            "delete" => Ok(Self::Delete),
            "noop" => Ok(Self::Noop),
            "add" | "" => Ok(Self::Add),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryRecord {
    pub id: i64,
    pub layer: MemoryLayer,
    pub scope: MemoryScope,
    pub scope_id: String,
    pub memory_type: String,
    pub title: String,
    pub content: String,
    pub source_type: MemorySourceType,
    pub source_ref: String,
    pub confidence: f64,
    pub status: MemoryStatus,
    pub tags: Vec<String>,
    pub links: Vec<String>,
    pub supersedes: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryEventRecord {
    pub id: i64,
    pub scope: MemoryScope,
    pub scope_id: String,
    pub event_type: String,
    pub content: String,
    pub source_ref: String,
    pub content_hash: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryLifecycleLogRecord {
    pub id: i64,
    pub action: LifecycleAction,
    pub target_type: String,
    pub target_id: Option<i64>,
    pub reason: String,
    pub model: Option<String>,
    pub source_ref: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemorySearchOptions {
    pub query: String,
    pub scope: Option<MemoryScope>,
    pub layer: Option<MemoryLayer>,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NewMemoryRecord {
    pub layer: MemoryLayer,
    pub scope: MemoryScope,
    pub scope_id: String,
    pub memory_type: String,
    pub title: String,
    pub content: String,
    pub source_type: MemorySourceType,
    pub source_ref: String,
    pub confidence: f64,
    pub status: MemoryStatus,
    pub tags: Vec<String>,
    pub links: Vec<String>,
    pub supersedes: Option<i64>,
    pub reason: String,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UpdateMemoryRecord {
    pub content: Option<String>,
    pub title: Option<String>,
    pub status: Option<MemoryStatus>,
    pub supersedes: Option<i64>,
    pub reason: String,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NewMemoryEvent {
    pub scope: MemoryScope,
    pub scope_id: String,
    pub event_type: String,
    pub content: String,
    pub source_ref: String,
}

#[derive(Debug, Clone)]
struct CachedVector {
    chunk_id: i64,
    doc_id: i64,
    file_path: PathBuf,
    embedding: Vec<f32>,
}

const INDEX_STATUS_READY: &str = "ready";
const INDEX_STATUS_PENDING: &str = "pending";
const INDEX_STATUS_FAILED: &str = "failed";
const DOCUMENT_SEARCH_PREVIEW_CHARS: usize = 2_000;
const DOCUMENT_SEARCH_SNIPPET_CHARS: usize = 180;
const DOCUMENT_SEARCH_MAX_SNIPPETS: usize = 8;
const DOCUMENT_SEARCH_SYMBOL_CHARS: usize = 6_000;

/// 向量存储统一抽象。
#[allow(async_fn_in_trait)]
pub trait VectorStore: Send + Sync {
    async fn insert_chunks(
        &self,
        chunks: Vec<DocumentChunk>,
        embeddings: Vec<Vec<f32>>,
    ) -> Result<(), StorageError>;

    async fn search_similar(
        &self,
        query_embedding: Vec<f32>,
        top_k: usize,
    ) -> Result<Vec<(DocumentChunk, f32)>, StorageError>;
}

/// 基于 RwLock 的内存向量存储实现。
#[derive(Debug, Default)]
pub struct InMemoryStore {
    records: RwLock<Vec<StoredVectorRecord>>,
}

mod document;
mod document_graph;
mod document_index;
mod memory;
mod schema;
mod schema_utils;
mod search;
mod store;
mod text;
mod text_query;
mod text_score;
mod vector;

pub(crate) use schema::*;
pub(crate) use schema_utils::*;
pub(crate) use text::*;
pub(crate) use text_query::*;
pub(crate) use text_score::*;

#[derive(Debug)]
pub struct SqliteStore {
    conn: Mutex<Connection>,
    cache: RwLock<Vec<CachedVector>>,
}

#[cfg(test)]
mod tests;

/// 存储层错误定义。
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("向量数量与文本块数量不一致: chunks={chunks}, embeddings={embeddings}")]
    LengthMismatch { chunks: usize, embeddings: usize },

    #[error("插入批次为空，无法写入数据库")]
    EmptyChunks,

    #[error("同一批次出现多个文件路径，拒绝写入")]
    MixedFilePathInBatch,

    #[error("chunk_id 不存在: {chunk_id}")]
    ChunkNotFound { chunk_id: i64 },

    #[error("chunk_index 超出可存储范围: {chunk_index}")]
    ChunkIndexOverflow { chunk_index: usize },

    #[error("数据库中的 chunk_index 非法: {raw}")]
    InvalidChunkIndex { raw: i64 },

    #[error("SQLite 连接锁已损坏: {0}")]
    LockPoisoned(&'static str),

    #[error("数据库当前被占用，请稍后重试: {0}")]
    DatabaseLocked(#[source] rusqlite::Error),

    #[error("SQLite 操作失败: {0}")]
    Sqlite(#[source] rusqlite::Error),

    #[error("JSON 序列化/反序列化失败: {0}")]
    SerdeJson(#[source] serde_json::Error),

    #[error("Embedding 序列化失败: {0}")]
    SerializeEmbedding(#[source] bincode::Error),

    #[error("Embedding 反序列化失败: {0}")]
    DeserializeEmbedding(#[source] bincode::Error),

    #[error("系统时间异常: {0}")]
    Clock(#[source] std::time::SystemTimeError),

    #[error("时间戳溢出")]
    TimestampOverflow,

    #[error("字段 {field} 超出可存储范围: {value}")]
    CountOverflow { field: &'static str, value: usize },

    #[error("IO 操作失败: {0}")]
    Io(#[source] std::io::Error),

    #[error("统计结果异常，表 {table} 出现负数计数: {count}")]
    NegativeCount { table: &'static str, count: i64 },

    #[error("缺少 catalog 记录: {file_path}")]
    MissingCatalogEntry { file_path: String },
}
