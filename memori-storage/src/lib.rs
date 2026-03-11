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

impl InMemoryStore {
    pub fn new() -> Self {
        Self {
            records: RwLock::new(Vec::new()),
        }
    }

    /// 用于调试/测试的记录总数。
    pub async fn len(&self) -> usize {
        self.records.read().await.len()
    }

    /// 与 `len` 成对提供，满足 clippy `len_without_is_empty` 约束。
    pub async fn is_empty(&self) -> bool {
        self.records.read().await.is_empty()
    }
}

impl VectorStore for InMemoryStore {
    async fn insert_chunks(
        &self,
        chunks: Vec<DocumentChunk>,
        embeddings: Vec<Vec<f32>>,
    ) -> Result<(), StorageError> {
        let chunk_count = chunks.len();
        let embedding_count = embeddings.len();

        if chunk_count != embedding_count {
            return Err(StorageError::LengthMismatch {
                chunks: chunk_count,
                embeddings: embedding_count,
            });
        }

        let mut guard = self.records.write().await;
        for (chunk, embedding) in chunks.into_iter().zip(embeddings.into_iter()) {
            guard.push(StoredVectorRecord { chunk, embedding });
        }

        info!(
            inserted = chunk_count,
            total_vectors = guard.len(),
            "成功存入 {} 条向量数据",
            chunk_count
        );

        Ok(())
    }

    async fn search_similar(
        &self,
        query_embedding: Vec<f32>,
        top_k: usize,
    ) -> Result<Vec<(DocumentChunk, f32)>, StorageError> {
        if top_k == 0 || query_embedding.is_empty() {
            return Ok(Vec::new());
        }

        let guard = self.records.read().await;
        let mut scored: Vec<(DocumentChunk, f32)> = guard
            .iter()
            .map(|record| {
                let score = cosine_similarity(&query_embedding, &record.embedding);
                (record.chunk.clone(), score)
            })
            .collect();

        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        scored.truncate(top_k);
        Ok(scored)
    }
}

/// SQLite 持久化存储实现。
#[derive(Debug)]
pub struct SqliteStore {
    conn: Mutex<Connection>,
    cache: RwLock<Vec<CachedVector>>,
}

impl SqliteStore {
    /// 打开数据库并初始化表结构。
    pub fn new(db_path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let db_path = db_path.as_ref();
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(StorageError::Io)?;
        }

        let conn = Connection::open(db_path).map_err(map_sqlite_error)?;
        initialize_schema(&conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
            cache: RwLock::new(Vec::new()),
        })
    }

    pub async fn read_index_metadata(&self) -> Result<IndexMetadata, StorageError> {
        let conn_guard = self.lock_conn()?;
        read_index_metadata_from_conn(&conn_guard)
    }

    pub async fn mark_rebuild_required(
        &self,
        reason: impl Into<String>,
    ) -> Result<(), StorageError> {
        let conn_guard = self.lock_conn()?;
        let reason = reason.into();
        set_metadata_value(
            &conn_guard,
            METADATA_KEY_REBUILD_STATE,
            RebuildState::Required.as_str(),
        )?;
        set_metadata_value(&conn_guard, METADATA_KEY_REBUILD_REASON, reason.trim())?;
        Ok(())
    }

    pub async fn begin_full_rebuild(&self, reason: impl Into<String>) -> Result<(), StorageError> {
        let conn_guard = self.lock_conn()?;
        let reason = reason.into();
        set_metadata_value(
            &conn_guard,
            METADATA_KEY_REBUILD_STATE,
            RebuildState::Rebuilding.as_str(),
        )?;
        set_metadata_value(&conn_guard, METADATA_KEY_REBUILD_REASON, reason.trim())?;
        Ok(())
    }

    pub async fn finish_full_rebuild(&self) -> Result<(), StorageError> {
        let now = current_unix_timestamp_secs()?;
        let conn_guard = self.lock_conn()?;
        set_metadata_value(
            &conn_guard,
            METADATA_KEY_INDEX_FORMAT_VERSION,
            &INDEX_FORMAT_VERSION.to_string(),
        )?;
        set_metadata_value(
            &conn_guard,
            METADATA_KEY_PARSER_FORMAT_VERSION,
            &PARSER_FORMAT_VERSION.to_string(),
        )?;
        set_metadata_value(
            &conn_guard,
            METADATA_KEY_REBUILD_STATE,
            RebuildState::Ready.as_str(),
        )?;
        set_metadata_value(&conn_guard, METADATA_KEY_REBUILD_REASON, "")?;
        set_metadata_value(&conn_guard, METADATA_KEY_LAST_REBUILD_AT, &now.to_string())?;
        Ok(())
    }

    pub async fn purge_all_index_data(&self) -> Result<(), StorageError> {
        {
            let mut conn_guard = self.lock_conn()?;
            let tx = conn_guard.transaction().map_err(map_sqlite_error)?;
            tx.execute("DELETE FROM graph_task_queue", [])
                .map_err(map_sqlite_error)?;
            tx.execute("DELETE FROM chunk_nodes", [])
                .map_err(map_sqlite_error)?;
            tx.execute("DELETE FROM edges", [])
                .map_err(map_sqlite_error)?;
            tx.execute("DELETE FROM nodes", [])
                .map_err(map_sqlite_error)?;
            tx.execute("DELETE FROM chunks_fts", [])
                .map_err(map_sqlite_error)?;
            tx.execute("DELETE FROM documents_fts", [])
                .map_err(map_sqlite_error)?;
            tx.execute("DELETE FROM file_index_state", [])
                .map_err(map_sqlite_error)?;
            tx.execute("DELETE FROM documents", [])
                .map_err(map_sqlite_error)?;
            tx.execute("DELETE FROM file_catalog", [])
                .map_err(map_sqlite_error)?;
            tx.commit().map_err(map_sqlite_error)?;
        }

        let mut cache_guard = self.cache.write().await;
        cache_guard.clear();
        Ok(())
    }

    /// 冷启动时从 DB 读取全部向量到内存缓存。
    /// 返回值为加载到缓存中的向量条数。
    pub async fn load_from_db(&self) -> Result<usize, StorageError> {
        let metadata = self.read_index_metadata().await?;
        if metadata.rebuild_state != RebuildState::Ready {
            let mut cache_guard = self.cache.write().await;
            cache_guard.clear();
            return Ok(0);
        }

        let loaded = {
            let conn_guard = self.lock_conn()?;
            let mut stmt = conn_guard
                .prepare(
                    "SELECT c.id, c.doc_id, c.embedding_blob, d.file_path
                     FROM chunks c
                     INNER JOIN documents d ON d.id = c.doc_id
                     INNER JOIN file_catalog fc ON fc.file_path = d.file_path
                     WHERE fc.removed_at IS NULL",
                )
                .map_err(map_sqlite_error)?;

            let mut rows = stmt.query([]).map_err(map_sqlite_error)?;
            let mut loaded = Vec::new();

            while let Some(row) = rows.next().map_err(map_sqlite_error)? {
                let chunk_id: i64 = row.get(0).map_err(map_sqlite_error)?;
                let doc_id: i64 = row.get(1).map_err(map_sqlite_error)?;
                let blob: Vec<u8> = row.get(2).map_err(map_sqlite_error)?;
                let file_path: String = row.get(3).map_err(map_sqlite_error)?;
                let embedding: Vec<f32> =
                    bincode::deserialize(&blob).map_err(StorageError::DeserializeEmbedding)?;

                loaded.push(CachedVector {
                    chunk_id,
                    doc_id,
                    file_path: PathBuf::from(file_path),
                    embedding,
                });
            }

            loaded
        };

        let mut cache_guard = self.cache.write().await;
        *cache_guard = loaded;

        Ok(cache_guard.len())
    }

    /// 通过 file_path + chunk_index 定位 chunks.id。
    pub async fn resolve_chunk_id(
        &self,
        file_path: &Path,
        chunk_index: usize,
    ) -> Result<Option<i64>, StorageError> {
        let chunk_index = i64::try_from(chunk_index)
            .map_err(|_| StorageError::ChunkIndexOverflow { chunk_index })?;

        let file_path_text = normalize_storage_file_path_text(file_path);
        let conn_guard = self.lock_conn()?;

        let chunk_id = conn_guard
            .query_row(
                "SELECT c.id
                 FROM chunks c
                 INNER JOIN documents d ON d.id = c.doc_id
                 INNER JOIN file_catalog fc ON fc.file_path = d.file_path
                 WHERE d.file_path = ?1
                   AND c.chunk_index = ?2
                   AND fc.removed_at IS NULL",
                params![file_path_text, chunk_index],
                |row| row.get(0),
            )
            .optional()
            .map_err(map_sqlite_error)?;

        Ok(chunk_id)
    }

    pub async fn upsert_catalog_entry(
        &self,
        file_path: &Path,
        watch_root: Option<&Path>,
        file_size: i64,
        mtime_secs: i64,
    ) -> Result<CatalogEntry, StorageError> {
        let file_path_text = normalize_storage_file_path_text(file_path);
        let relative_path = build_relative_path(file_path, watch_root);
        let parent_dir = Path::new(&relative_path)
            .parent()
            .and_then(|path| path.to_str())
            .unwrap_or_default()
            .replace('\\', "/");
        let file_name = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();
        let file_ext = file_path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let discovered_at = current_unix_timestamp_secs()?;
        let conn_guard = self.lock_conn()?;
        conn_guard
            .execute(
                "INSERT INTO file_catalog(
                    file_path, relative_path, parent_dir, file_name, file_ext, file_size, mtime_secs, discovered_at, removed_at
                 )
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL)
                 ON CONFLICT(file_path) DO UPDATE SET
                    relative_path = excluded.relative_path,
                    parent_dir = excluded.parent_dir,
                    file_name = excluded.file_name,
                    file_ext = excluded.file_ext,
                    file_size = excluded.file_size,
                    mtime_secs = excluded.mtime_secs,
                    discovered_at = excluded.discovered_at,
                    removed_at = NULL",
                params![
                    file_path_text,
                    relative_path,
                    parent_dir,
                    file_name,
                    file_ext,
                    file_size,
                    mtime_secs,
                    discovered_at
                ],
            )
            .map_err(map_sqlite_error)?;

        Ok(CatalogEntry {
            file_path: file_path_text,
            relative_path,
            parent_dir,
            file_name,
            file_ext,
            file_size,
            mtime_secs,
            discovered_at,
            removed_at: None,
        })
    }

    pub async fn get_catalog_entry(
        &self,
        file_path: &Path,
    ) -> Result<Option<CatalogEntry>, StorageError> {
        let file_path_text = normalize_storage_file_path_text(file_path);
        let conn_guard = self.lock_conn()?;
        conn_guard
            .query_row(
                "SELECT file_path, relative_path, parent_dir, file_name, file_ext, file_size, mtime_secs, discovered_at, removed_at
                 FROM file_catalog
                 WHERE file_path = ?1",
                params![file_path_text],
                |row| {
                    Ok(CatalogEntry {
                        file_path: row.get(0)?,
                        relative_path: row.get(1)?,
                        parent_dir: row.get(2)?,
                        file_name: row.get(3)?,
                        file_ext: row.get(4)?,
                        file_size: row.get(5)?,
                        mtime_secs: row.get(6)?,
                        discovered_at: row.get(7)?,
                        removed_at: row.get(8)?,
                    })
                },
            )
            .optional()
            .map_err(map_sqlite_error)
    }

    pub async fn mark_catalog_removed(&self, file_path: &Path) -> Result<(), StorageError> {
        let file_path_text = normalize_storage_file_path_text(file_path);
        let removed_at = current_unix_timestamp_secs()?;
        let conn_guard = self.lock_conn()?;
        conn_guard
            .execute(
                "UPDATE file_catalog SET removed_at = ?2 WHERE file_path = ?1",
                params![file_path_text, removed_at],
            )
            .map_err(map_sqlite_error)?;
        Ok(())
    }

    pub async fn mark_file_index_pending(
        &self,
        file_path: &Path,
        file_size: i64,
        mtime_secs: i64,
        content_hash: &str,
    ) -> Result<(), StorageError> {
        self.upsert_file_index_state_internal(
            file_path,
            file_size,
            mtime_secs,
            content_hash,
            INDEX_STATUS_PENDING,
            None,
        )
        .await
    }

    pub async fn mark_file_index_failed(
        &self,
        file_path: &Path,
        file_size: i64,
        mtime_secs: i64,
        content_hash: &str,
        last_error: &str,
    ) -> Result<(), StorageError> {
        self.upsert_file_index_state_internal(
            file_path,
            file_size,
            mtime_secs,
            content_hash,
            INDEX_STATUS_FAILED,
            Some(last_error),
        )
        .await
    }

    pub async fn get_document_by_file_path(
        &self,
        file_path: &Path,
    ) -> Result<Option<DocumentRecord>, StorageError> {
        let file_path_text = normalize_storage_file_path_text(file_path);
        let conn_guard = self.lock_conn()?;
        conn_guard
            .query_row(
                "SELECT d.id, d.file_path, d.relative_path, d.file_name, d.file_ext, d.last_modified, d.indexed_at,
                        d.chunk_count, d.content_char_count, d.heading_catalog_text, d.document_search_text
                 FROM documents d
                 INNER JOIN file_catalog fc ON fc.file_path = d.file_path
                 WHERE d.file_path = ?1
                   AND fc.removed_at IS NULL",
                params![file_path_text],
                map_document_record,
            )
            .optional()
            .map_err(map_sqlite_error)
    }

    pub async fn get_chunks_by_doc_id(
        &self,
        doc_id: i64,
    ) -> Result<Vec<ChunkRecord>, StorageError> {
        let conn_guard = self.lock_conn()?;
        let mut stmt = conn_guard
            .prepare(
                "SELECT id, doc_id, chunk_index, content, heading_path_json, block_kind, char_len
                 FROM chunks
                 WHERE doc_id = ?1
                 ORDER BY chunk_index ASC",
            )
            .map_err(map_sqlite_error)?;
        let rows = stmt
            .query_map(params![doc_id], map_chunk_record)
            .map_err(map_sqlite_error)?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(map_sqlite_error)?);
        }
        Ok(records)
    }

    pub async fn search_chunks_fts(
        &self,
        query: &str,
        top_k: usize,
        scope_paths: &[PathBuf],
    ) -> Result<Vec<FtsChunkMatch>, StorageError> {
        let Some(match_query) = build_fts_match_query(query, FtsQueryMode::BroadOr) else {
            return Ok(Vec::new());
        };
        if top_k == 0 {
            return Ok(Vec::new());
        }

        let scope_matchers = build_scope_matchers(scope_paths);
        let fetch_limit = i64::try_from(top_k.saturating_mul(4).max(top_k)).unwrap_or(i64::MAX);
        let conn_guard = self.lock_conn()?;
        let mut stmt = conn_guard
            .prepare(
                "SELECT c.id,
                        c.doc_id,
                        d.file_path,
                        d.relative_path,
                        d.file_name,
                        c.chunk_index,
                        -bm25(chunks_fts) AS score,
                        c.content,
                        c.heading_path_json,
                        c.block_kind
                 FROM chunks_fts
                 INNER JOIN chunks c ON c.id = chunks_fts.chunk_id
                 INNER JOIN documents d ON d.id = c.doc_id
                 INNER JOIN file_catalog fc ON fc.file_path = d.file_path
                 WHERE chunks_fts MATCH ?1
                   AND fc.removed_at IS NULL
                 ORDER BY bm25(chunks_fts)
                 LIMIT ?2",
            )
            .map_err(map_sqlite_error)?;
        let rows = stmt
            .query_map(params![match_query, fetch_limit], |row| {
                let heading_path_json: String = row.get(8)?;
                Ok(FtsChunkMatch {
                    chunk_id: row.get(0)?,
                    doc_id: row.get(1)?,
                    file_path: row.get(2)?,
                    relative_path: row.get(3)?,
                    file_name: row.get(4)?,
                    chunk_index: usize::try_from(row.get::<_, i64>(5)?).unwrap_or_default(),
                    score: row.get(6)?,
                    content: row.get(7)?,
                    heading_path: serde_json::from_str(&heading_path_json).unwrap_or_default(),
                    block_kind: row.get(9)?,
                })
            })
            .map_err(map_sqlite_error)?;

        let mut matches = Vec::new();
        for row in rows {
            let entry = row.map_err(map_sqlite_error)?;
            if matches_scopes(Path::new(&entry.file_path), &scope_matchers) {
                matches.push(entry);
                if matches.len() >= top_k {
                    break;
                }
            }
        }

        Ok(matches)
    }

    pub async fn search_chunks_fts_strict(
        &self,
        query: &str,
        top_k: usize,
        scope_paths: &[PathBuf],
    ) -> Result<Vec<FtsChunkMatch>, StorageError> {
        let Some(match_query) = build_fts_match_query(query, FtsQueryMode::StrictAnd) else {
            return Ok(Vec::new());
        };
        if top_k == 0 {
            return Ok(Vec::new());
        }

        let scope_matchers = build_scope_matchers(scope_paths);
        let fetch_limit = i64::try_from(top_k.saturating_mul(6).max(top_k)).unwrap_or(i64::MAX);
        let conn_guard = self.lock_conn()?;
        let mut stmt = conn_guard
            .prepare(
                "SELECT c.id,
                        c.doc_id,
                        d.file_path,
                        d.relative_path,
                        d.file_name,
                        c.chunk_index,
                        -bm25(chunks_fts) AS score,
                        c.content,
                        c.heading_path_json,
                        c.block_kind
                 FROM chunks_fts
                 INNER JOIN chunks c ON c.id = chunks_fts.chunk_id
                 INNER JOIN documents d ON d.id = c.doc_id
                 INNER JOIN file_catalog fc ON fc.file_path = d.file_path
                 WHERE chunks_fts MATCH ?1
                   AND fc.removed_at IS NULL
                 ORDER BY bm25(chunks_fts)
                 LIMIT ?2",
            )
            .map_err(map_sqlite_error)?;
        let rows = stmt
            .query_map(params![match_query, fetch_limit], |row| {
                let heading_path_json: String = row.get(8)?;
                Ok(FtsChunkMatch {
                    chunk_id: row.get(0)?,
                    doc_id: row.get(1)?,
                    file_path: row.get(2)?,
                    relative_path: row.get(3)?,
                    file_name: row.get(4)?,
                    chunk_index: usize::try_from(row.get::<_, i64>(5)?).unwrap_or_default(),
                    score: row.get(6)?,
                    content: row.get(7)?,
                    heading_path: serde_json::from_str(&heading_path_json).unwrap_or_default(),
                    block_kind: row.get(9)?,
                })
            })
            .map_err(map_sqlite_error)?;

        let mut matches = Vec::new();
        for row in rows {
            let entry = row.map_err(map_sqlite_error)?;
            if matches_scopes(Path::new(&entry.file_path), &scope_matchers) {
                matches.push(entry);
                if matches.len() >= top_k {
                    break;
                }
            }
        }

        Ok(matches)
    }

    pub async fn search_documents_fts(
        &self,
        query: &str,
        top_k: usize,
        scope_paths: &[PathBuf],
    ) -> Result<Vec<FtsDocumentMatch>, StorageError> {
        let Some(match_query) = build_fts_match_query(query, FtsQueryMode::BroadOr) else {
            return Ok(Vec::new());
        };
        if top_k == 0 {
            return Ok(Vec::new());
        }

        let scope_matchers = build_scope_matchers(scope_paths);
        let fetch_limit = i64::try_from(top_k.saturating_mul(4).max(top_k)).unwrap_or(i64::MAX);
        let conn_guard = self.lock_conn()?;
        let mut stmt = conn_guard
            .prepare(
                "SELECT d.id,
                        d.file_path,
                        d.relative_path,
                        d.file_name,
                        -bm25(documents_fts) AS score,
                        d.heading_catalog_text,
                        d.document_search_text
                 FROM documents_fts
                 INNER JOIN documents d ON d.id = documents_fts.doc_id
                 INNER JOIN file_catalog fc ON fc.file_path = d.file_path
                 WHERE documents_fts MATCH ?1
                   AND fc.removed_at IS NULL
                 ORDER BY bm25(documents_fts)
                 LIMIT ?2",
            )
            .map_err(map_sqlite_error)?;
        let rows = stmt
            .query_map(params![match_query, fetch_limit], |row| {
                Ok(FtsDocumentMatch {
                    doc_id: row.get(0)?,
                    file_path: row.get(1)?,
                    relative_path: row.get(2)?,
                    file_name: row.get(3)?,
                    score: row.get(4)?,
                    heading_catalog_text: row.get(5)?,
                    document_search_text: row.get(6)?,
                })
            })
            .map_err(map_sqlite_error)?;

        let mut matches = Vec::new();
        for row in rows {
            let entry = row.map_err(map_sqlite_error)?;
            if matches_scopes(Path::new(&entry.file_path), &scope_matchers) {
                matches.push(entry);
                if matches.len() >= top_k {
                    break;
                }
            }
        }

        Ok(matches)
    }

    pub async fn search_documents_fts_strict(
        &self,
        query: &str,
        top_k: usize,
        scope_paths: &[PathBuf],
    ) -> Result<Vec<FtsDocumentMatch>, StorageError> {
        let Some(match_query) = build_fts_match_query(query, FtsQueryMode::StrictAnd) else {
            return Ok(Vec::new());
        };
        if top_k == 0 {
            return Ok(Vec::new());
        }

        let scope_matchers = build_scope_matchers(scope_paths);
        let fetch_limit = i64::try_from(top_k.saturating_mul(6).max(top_k)).unwrap_or(i64::MAX);
        let conn_guard = self.lock_conn()?;
        let mut stmt = conn_guard
            .prepare(
                "SELECT d.id,
                        d.file_path,
                        d.relative_path,
                        d.file_name,
                        -bm25(documents_fts) AS score,
                        d.heading_catalog_text,
                        d.document_search_text
                 FROM documents_fts
                 INNER JOIN documents d ON d.id = documents_fts.doc_id
                 INNER JOIN file_catalog fc ON fc.file_path = d.file_path
                 WHERE documents_fts MATCH ?1
                   AND fc.removed_at IS NULL
                 ORDER BY bm25(documents_fts)
                 LIMIT ?2",
            )
            .map_err(map_sqlite_error)?;
        let rows = stmt
            .query_map(params![match_query, fetch_limit], |row| {
                Ok(FtsDocumentMatch {
                    doc_id: row.get(0)?,
                    file_path: row.get(1)?,
                    relative_path: row.get(2)?,
                    file_name: row.get(3)?,
                    score: row.get(4)?,
                    heading_catalog_text: row.get(5)?,
                    document_search_text: row.get(6)?,
                })
            })
            .map_err(map_sqlite_error)?;

        let mut matches = Vec::new();
        for row in rows {
            let entry = row.map_err(map_sqlite_error)?;
            if matches_scopes(Path::new(&entry.file_path), &scope_matchers) {
                matches.push(entry);
                if matches.len() >= top_k {
                    break;
                }
            }
        }

        Ok(matches)
    }

    pub async fn search_documents_signal(
        &self,
        query: &str,
        top_k: usize,
        scope_paths: &[PathBuf],
    ) -> Result<Vec<DocumentSignalMatch>, StorageError> {
        if top_k == 0 {
            return Ok(Vec::new());
        }

        let signal_terms = extract_signal_terms(query);
        if signal_terms.is_empty() {
            return Ok(Vec::new());
        }

        let scope_matchers = build_scope_matchers(scope_paths);
        let conn_guard = self.lock_conn()?;
        let mut stmt = conn_guard
            .prepare(
                "SELECT d.file_path,
                        d.relative_path,
                        d.file_name,
                        d.heading_catalog_text,
                        d.document_search_text
                 FROM documents d
                 INNER JOIN file_catalog fc ON fc.file_path = d.file_path
                 WHERE fc.removed_at IS NULL",
            )
            .map_err(map_sqlite_error)?;

        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(map_sqlite_error)?;

        let mut matches = Vec::new();
        for row in rows {
            let (file_path, relative_path, file_name, heading_catalog_text, document_search_text) =
                row.map_err(map_sqlite_error)?;
            if !matches_scopes(Path::new(&file_path), &scope_matchers) {
                continue;
            }

            let Some((score, matched_fields)) = score_document_signal_match(
                &signal_terms,
                &file_name,
                &relative_path,
                &heading_catalog_text,
                &document_search_text,
            ) else {
                continue;
            };

            matches.push(DocumentSignalMatch {
                file_path,
                relative_path,
                file_name,
                matched_fields,
                score,
            });
        }

        matches.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.file_name.len().cmp(&b.file_name.len()))
                .then_with(|| a.relative_path.cmp(&b.relative_path))
        });
        matches.truncate(top_k);
        Ok(matches)
    }

    pub async fn search_documents_phrase_signal(
        &self,
        query: &str,
        top_k: usize,
        scope_paths: &[PathBuf],
    ) -> Result<Vec<DocumentSignalMatch>, StorageError> {
        if top_k == 0 {
            return Ok(Vec::new());
        }

        let phrase_terms = extract_phrase_signal_terms(query);
        if phrase_terms.is_empty() {
            return Ok(Vec::new());
        }

        let scope_matchers = build_scope_matchers(scope_paths);
        let conn_guard = self.lock_conn()?;
        let mut stmt = conn_guard
            .prepare(
                "SELECT d.file_path,
                        d.relative_path,
                        d.file_name,
                        d.heading_catalog_text,
                        d.document_search_text
                 FROM documents d
                 INNER JOIN file_catalog fc ON fc.file_path = d.file_path
                 WHERE fc.removed_at IS NULL",
            )
            .map_err(map_sqlite_error)?;

        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(map_sqlite_error)?;

        let mut matches = Vec::new();
        for row in rows {
            let (file_path, relative_path, file_name, heading_catalog_text, document_search_text) =
                row.map_err(map_sqlite_error)?;
            if !matches_scopes(Path::new(&file_path), &scope_matchers) {
                continue;
            }

            let Some((score, matched_fields)) = score_document_phrase_signal_match(
                &phrase_terms,
                &file_name,
                &relative_path,
                &heading_catalog_text,
                &document_search_text,
            ) else {
                continue;
            };

            matches.push(DocumentSignalMatch {
                file_path,
                relative_path,
                file_name,
                matched_fields,
                score,
            });
        }

        matches.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.file_name.len().cmp(&b.file_name.len()))
                .then_with(|| a.relative_path.cmp(&b.relative_path))
        });
        matches.truncate(top_k);
        Ok(matches)
    }

    pub async fn replace_document_index(
        &self,
        file_path: &Path,
        watch_root: Option<&Path>,
        last_modified: i64,
        content_hash: &str,
        chunks: Vec<DocumentChunk>,
        embeddings: Vec<Vec<f32>>,
    ) -> Result<(), StorageError> {
        let chunk_count = chunks.len();
        let embedding_count = embeddings.len();
        if chunk_count == 0 {
            return Err(StorageError::EmptyChunks);
        }
        if chunk_count != embedding_count {
            return Err(StorageError::LengthMismatch {
                chunks: chunk_count,
                embeddings: embedding_count,
            });
        }

        let normalized_file_path = normalize_storage_file_path_text(file_path);
        let catalog_entry = self
            .upsert_catalog_entry(
                file_path,
                watch_root,
                inferred_file_size(&chunks),
                last_modified,
            )
            .await?;
        let indexed_at = current_unix_timestamp_secs()?;
        let heading_catalog_text = build_heading_catalog_text(&chunks);
        let document_search_text =
            build_document_search_text(&catalog_entry, &heading_catalog_text, &chunks);
        let content_char_count: usize = chunks
            .iter()
            .map(|chunk| chunk.content.chars().count())
            .sum();
        let normalized_file_path_for_document = normalized_file_path.clone();
        let normalized_file_path_for_lookup = normalized_file_path.clone();
        let normalized_file_path_for_chunks_fts = normalized_file_path.clone();
        let normalized_file_path_for_documents_fts = normalized_file_path.clone();
        let normalized_file_path_for_index_state = normalized_file_path.clone();
        let doc_relative_path = catalog_entry.relative_path.clone();
        let doc_file_name = catalog_entry.file_name.clone();
        let doc_file_ext = catalog_entry.file_ext.clone();
        let catalog_file_name = catalog_entry.file_name.clone();
        let catalog_relative_path = catalog_entry.relative_path.clone();
        let heading_catalog_text_for_document = heading_catalog_text.clone();
        let heading_catalog_text_for_documents_fts = heading_catalog_text.clone();
        let document_search_text_for_document = document_search_text.clone();
        let document_search_text_for_documents_fts = document_search_text.clone();

        let (doc_id, inserted_cache_rows) = {
            let mut conn_guard = self.lock_conn()?;
            let tx = conn_guard.transaction().map_err(map_sqlite_error)?;

            tx.execute(
                "INSERT INTO documents(
                    file_path, relative_path, file_name, file_ext, last_modified, indexed_at,
                    chunk_count, content_char_count, heading_catalog_text, document_search_text
                 )
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(file_path) DO UPDATE SET
                    relative_path = excluded.relative_path,
                    file_name = excluded.file_name,
                    file_ext = excluded.file_ext,
                    last_modified = excluded.last_modified,
                    indexed_at = excluded.indexed_at,
                    chunk_count = excluded.chunk_count,
                    content_char_count = excluded.content_char_count,
                    heading_catalog_text = excluded.heading_catalog_text,
                    document_search_text = excluded.document_search_text",
                params![
                    normalized_file_path_for_document,
                    doc_relative_path,
                    doc_file_name,
                    doc_file_ext,
                    last_modified,
                    indexed_at,
                    i64::try_from(chunk_count).map_err(|_| StorageError::CountOverflow {
                        field: "documents.chunk_count",
                        value: chunk_count,
                    })?,
                    i64::try_from(content_char_count).map_err(|_| StorageError::CountOverflow {
                        field: "documents.content_char_count",
                        value: content_char_count,
                    })?,
                    heading_catalog_text_for_document,
                    document_search_text_for_document
                ],
            )
            .map_err(map_sqlite_error)?;

            let doc_id: i64 = tx
                .query_row(
                    "SELECT id FROM documents WHERE file_path = ?1",
                    params![normalized_file_path_for_lookup],
                    |row| row.get(0),
                )
                .map_err(map_sqlite_error)?;

            tx.execute("DELETE FROM chunks_fts WHERE doc_id = ?1", params![doc_id])
                .map_err(map_sqlite_error)?;
            tx.execute(
                "DELETE FROM documents_fts WHERE doc_id = ?1",
                params![doc_id],
            )
            .map_err(map_sqlite_error)?;
            tx.execute("DELETE FROM chunks WHERE doc_id = ?1", params![doc_id])
                .map_err(map_sqlite_error)?;

            let mut inserted = Vec::with_capacity(chunk_count);
            for (chunk, embedding) in chunks.into_iter().zip(embeddings.into_iter()) {
                let chunk_index = i64::try_from(chunk.chunk_index).map_err(|_| {
                    StorageError::ChunkIndexOverflow {
                        chunk_index: chunk.chunk_index,
                    }
                })?;
                let heading_path_json =
                    serde_json::to_string(&chunk.heading_path).map_err(StorageError::SerdeJson)?;
                let block_kind = chunk_block_kind_to_storage(chunk.block_kind);
                let char_len = i64::try_from(chunk.content.chars().count()).map_err(|_| {
                    StorageError::CountOverflow {
                        field: "chunks.char_len",
                        value: chunk.content.chars().count(),
                    }
                })?;
                let embedding_blob =
                    bincode::serialize(&embedding).map_err(StorageError::SerializeEmbedding)?;

                tx.execute(
                    "INSERT INTO chunks(
                        doc_id, chunk_index, content, heading_path_json, block_kind, embedding_blob, char_len
                     )
                     VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        doc_id,
                        chunk_index,
                        chunk.content,
                        heading_path_json,
                        block_kind,
                        embedding_blob,
                        char_len
                    ],
                )
                .map_err(map_sqlite_error)?;

                let chunk_id = tx.last_insert_rowid();
                let heading_text = chunk.heading_path.join(" / ");
                tx.execute(
                    "INSERT INTO chunks_fts(
                        content, heading_text, file_name, relative_path, chunk_id, doc_id, file_path
                     )
                     VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        chunk.content,
                        heading_text,
                        catalog_file_name.clone(),
                        catalog_relative_path.clone(),
                        chunk_id,
                        doc_id,
                        normalized_file_path_for_chunks_fts.clone()
                    ],
                )
                .map_err(map_sqlite_error)?;

                inserted.push(CachedVector {
                    chunk_id,
                    doc_id,
                    file_path: PathBuf::from(normalized_file_path.clone()),
                    embedding,
                });
            }

            tx.execute(
                "INSERT INTO documents_fts(
                    search_text, file_name, relative_path, heading_catalog_text, doc_id, file_path
                 )
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    document_search_text_for_documents_fts,
                    catalog_file_name,
                    catalog_relative_path,
                    heading_catalog_text_for_documents_fts,
                    doc_id,
                    normalized_file_path_for_documents_fts
                ],
            )
            .map_err(map_sqlite_error)?;

            tx.execute(
                "INSERT INTO file_index_state(
                    file_path, file_size, mtime_secs, content_hash, indexed_at,
                    index_status, last_error, parser_format_version, index_format_version
                 )
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7, ?8)
                 ON CONFLICT(file_path) DO UPDATE SET
                    file_size = excluded.file_size,
                    mtime_secs = excluded.mtime_secs,
                    content_hash = excluded.content_hash,
                    indexed_at = excluded.indexed_at,
                    index_status = excluded.index_status,
                    last_error = excluded.last_error,
                    parser_format_version = excluded.parser_format_version,
                    index_format_version = excluded.index_format_version",
                params![
                    normalized_file_path_for_index_state,
                    catalog_entry.file_size,
                    catalog_entry.mtime_secs,
                    content_hash,
                    indexed_at,
                    INDEX_STATUS_READY,
                    PARSER_FORMAT_VERSION,
                    INDEX_FORMAT_VERSION
                ],
            )
            .map_err(map_sqlite_error)?;

            tx.commit().map_err(map_sqlite_error)?;
            (doc_id, inserted)
        };

        let mut cache_guard = self.cache.write().await;
        cache_guard.retain(|item| item.doc_id != doc_id);
        cache_guard.extend(inserted_cache_rows);
        Ok(())
    }

    /// 将图谱数据写入数据库，并与 chunk_id 建立关联。
    ///
    /// 幂等保证：
    /// - `nodes` 使用 ON CONFLICT(id) DO UPDATE
    /// - `edges` 使用 ON CONFLICT(id) DO UPDATE
    /// - `chunk_nodes` 使用 INSERT OR IGNORE + 联合主键
    pub async fn insert_graph(
        &self,
        chunk_id: i64,
        nodes: Vec<GraphNode>,
        edges: Vec<GraphEdge>,
    ) -> Result<(), StorageError> {
        let conn_guard = self.lock_conn()?;
        let tx = conn_guard
            .unchecked_transaction()
            .map_err(map_sqlite_error)?;

        let exists: i64 = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM chunks WHERE id = ?1)",
                params![chunk_id],
                |row| row.get(0),
            )
            .map_err(map_sqlite_error)?;

        if exists == 0 {
            return Err(StorageError::ChunkNotFound { chunk_id });
        }

        let mut valid_nodes = 0usize;
        let mut available_node_ids = HashSet::new();
        for node in &nodes {
            if node.id.trim().is_empty()
                || node.label.trim().is_empty()
                || node.name.trim().is_empty()
            {
                continue;
            }

            tx.execute(
                "INSERT INTO nodes(id, label, name, description)
                 VALUES(?1, ?2, ?3, ?4)
                 ON CONFLICT(id) DO UPDATE SET
                   label = excluded.label,
                   name = excluded.name,
                   description = CASE
                     WHEN excluded.description IS NOT NULL AND excluded.description <> ''
                     THEN excluded.description
                     ELSE nodes.description
                   END",
                params![node.id, node.label, node.name, node.description],
            )
            .map_err(map_sqlite_error)?;

            tx.execute(
                "INSERT OR IGNORE INTO chunk_nodes(chunk_id, node_id) VALUES(?1, ?2)",
                params![chunk_id, node.id],
            )
            .map_err(map_sqlite_error)?;

            available_node_ids.insert(node.id.clone());
            valid_nodes += 1;
        }

        let mut candidate_edges: Vec<&GraphEdge> = Vec::new();
        let mut unresolved_node_ids = HashSet::new();
        for edge in &edges {
            if edge.id.trim().is_empty()
                || edge.source_node.trim().is_empty()
                || edge.target_node.trim().is_empty()
                || edge.relation.trim().is_empty()
            {
                continue;
            }
            if !available_node_ids.contains(&edge.source_node) {
                unresolved_node_ids.insert(edge.source_node.clone());
            }
            if !available_node_ids.contains(&edge.target_node) {
                unresolved_node_ids.insert(edge.target_node.clone());
            }
            candidate_edges.push(edge);
        }

        if !unresolved_node_ids.is_empty() {
            let mut ids: Vec<String> = unresolved_node_ids.into_iter().collect();
            ids.sort();
            let placeholders = make_placeholders(ids.len());
            let query = format!("SELECT id FROM nodes WHERE id IN ({})", placeholders);
            let mut stmt = tx.prepare(&query).map_err(map_sqlite_error)?;
            let rows = stmt
                .query_map(params_from_iter(ids.iter()), |row| row.get::<_, String>(0))
                .map_err(map_sqlite_error)?;
            for row in rows {
                available_node_ids.insert(row.map_err(map_sqlite_error)?);
            }
        }

        let mut valid_edges = 0usize;
        let mut skipped_edges = 0usize;
        for edge in candidate_edges {
            if !available_node_ids.contains(&edge.source_node)
                || !available_node_ids.contains(&edge.target_node)
            {
                skipped_edges += 1;
                continue;
            }

            tx.execute(
                "INSERT INTO edges(id, source_node, target_node, relation)
                 VALUES(?1, ?2, ?3, ?4)
                 ON CONFLICT(id) DO UPDATE SET
                   source_node = excluded.source_node,
                   target_node = excluded.target_node,
                   relation = excluded.relation",
                params![edge.id, edge.source_node, edge.target_node, edge.relation],
            )
            .map_err(map_sqlite_error)?;

            valid_edges += 1;
        }

        tx.commit().map_err(map_sqlite_error)?;

        info!(
            chunk_id = chunk_id,
            node_count = valid_nodes,
            edge_count = valid_edges,
            skipped_edge_count = skipped_edges,
            "图谱数据写入完成"
        );

        Ok(())
    }

    pub async fn get_file_index_state(
        &self,
        file_path: &Path,
    ) -> Result<Option<FileIndexState>, StorageError> {
        let file_path_text = normalize_storage_file_path_text(file_path);
        let conn_guard = self.lock_conn()?;
        let row = conn_guard
            .query_row(
                "SELECT file_path, file_size, mtime_secs, content_hash, indexed_at,
                        index_status, last_error, parser_format_version, index_format_version
                 FROM file_index_state
                 WHERE file_path = ?1",
                params![file_path_text],
                |row| {
                    Ok(FileIndexState {
                        file_path: row.get(0)?,
                        file_size: row.get(1)?,
                        mtime_secs: row.get(2)?,
                        content_hash: row.get(3)?,
                        indexed_at: row.get(4)?,
                        index_status: row.get(5)?,
                        last_error: row.get(6)?,
                        parser_format_version: row.get(7)?,
                        index_format_version: row.get(8)?,
                    })
                },
            )
            .optional()
            .map_err(map_sqlite_error)?;
        Ok(row)
    }

    pub async fn upsert_file_index_state(
        &self,
        file_path: &Path,
        file_size: i64,
        mtime_secs: i64,
        content_hash: &str,
    ) -> Result<(), StorageError> {
        self.upsert_file_index_state_internal(
            file_path,
            file_size,
            mtime_secs,
            content_hash,
            INDEX_STATUS_READY,
            None,
        )
        .await
    }

    async fn upsert_file_index_state_internal(
        &self,
        file_path: &Path,
        file_size: i64,
        mtime_secs: i64,
        content_hash: &str,
        index_status: &str,
        last_error: Option<&str>,
    ) -> Result<(), StorageError> {
        let file_path_text = normalize_storage_file_path_text(file_path);
        let now = current_unix_timestamp_secs()?;
        let conn_guard = self.lock_conn()?;
        conn_guard
            .execute(
                "INSERT INTO file_index_state(
                    file_path, file_size, mtime_secs, content_hash, indexed_at,
                    index_status, last_error, parser_format_version, index_format_version
                 )
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(file_path) DO UPDATE SET
                   file_size = excluded.file_size,
                   mtime_secs = excluded.mtime_secs,
                   content_hash = excluded.content_hash,
                   indexed_at = excluded.indexed_at,
                   index_status = excluded.index_status,
                   last_error = excluded.last_error,
                   parser_format_version = excluded.parser_format_version,
                   index_format_version = excluded.index_format_version",
                params![
                    file_path_text,
                    file_size,
                    mtime_secs,
                    content_hash,
                    now,
                    index_status,
                    last_error,
                    PARSER_FORMAT_VERSION,
                    INDEX_FORMAT_VERSION
                ],
            )
            .map_err(map_sqlite_error)?;
        Ok(())
    }

    pub async fn purge_file_path(&self, file_path: &Path) -> Result<bool, StorageError> {
        let file_path_text = normalize_storage_file_path_text(file_path);

        let (removed_doc_id, removed_state_rows, removed_doc_rows) = {
            let mut conn_guard = self.lock_conn()?;
            let tx = conn_guard.transaction().map_err(map_sqlite_error)?;

            let doc_id = tx
                .query_row(
                    "SELECT id FROM documents WHERE file_path = ?1",
                    params![file_path_text.clone()],
                    |row| row.get::<_, i64>(0),
                )
                .optional()
                .map_err(map_sqlite_error)?;

            if let Some(doc_id) = doc_id {
                tx.execute("DELETE FROM chunks_fts WHERE doc_id = ?1", params![doc_id])
                    .map_err(map_sqlite_error)?;
                tx.execute(
                    "DELETE FROM documents_fts WHERE doc_id = ?1",
                    params![doc_id],
                )
                .map_err(map_sqlite_error)?;
            }

            let removed_state_rows = tx
                .execute(
                    "DELETE FROM file_index_state WHERE file_path = ?1",
                    params![file_path_text.clone()],
                )
                .map_err(map_sqlite_error)?;

            let mut removed_doc_rows = 0usize;
            if let Some(doc_id) = doc_id {
                removed_doc_rows = tx
                    .execute("DELETE FROM documents WHERE id = ?1", params![doc_id])
                    .map_err(map_sqlite_error)?;
            }

            tx.execute(
                "UPDATE file_catalog SET removed_at = ?2 WHERE file_path = ?1",
                params![file_path_text.clone(), current_unix_timestamp_secs()?],
            )
            .map_err(map_sqlite_error)?;

            tx.commit().map_err(map_sqlite_error)?;
            (doc_id, removed_state_rows, removed_doc_rows)
        };

        if let Some(doc_id) = removed_doc_id {
            let mut cache_guard = self.cache.write().await;
            cache_guard.retain(|item| item.doc_id != doc_id);
        }

        Ok(removed_state_rows > 0 || removed_doc_rows > 0)
    }

    pub async fn purge_directory_path(&self, dir_path: &Path) -> Result<bool, StorageError> {
        let dir_path_text = normalize_storage_file_path_text(dir_path);
        let (like_prefix_slash, like_prefix_backslash) = directory_like_pattern(&dir_path_text);

        let (removed_doc_ids, removed_state_rows, removed_doc_rows) = {
            let mut conn_guard = self.lock_conn()?;
            let tx = conn_guard.transaction().map_err(map_sqlite_error)?;

            let mut doc_ids = Vec::new();
            {
                let mut stmt = tx
                    .prepare(
                        "SELECT id FROM documents
                         WHERE file_path = ?1
                            OR file_path LIKE ?2 ESCAPE '\\'
                            OR file_path LIKE ?3 ESCAPE '\\'",
                    )
                    .map_err(map_sqlite_error)?;
                let rows = stmt
                    .query_map(
                        params![
                            dir_path_text.clone(),
                            like_prefix_slash.clone(),
                            like_prefix_backslash.clone()
                        ],
                        |row| row.get::<_, i64>(0),
                    )
                    .map_err(map_sqlite_error)?;

                for row in rows {
                    doc_ids.push(row.map_err(map_sqlite_error)?);
                }
            }

            for doc_id in &doc_ids {
                tx.execute("DELETE FROM chunks_fts WHERE doc_id = ?1", params![doc_id])
                    .map_err(map_sqlite_error)?;
                tx.execute(
                    "DELETE FROM documents_fts WHERE doc_id = ?1",
                    params![doc_id],
                )
                .map_err(map_sqlite_error)?;
            }

            let removed_state_rows = tx
                .execute(
                    "DELETE FROM file_index_state
                     WHERE file_path = ?1
                        OR file_path LIKE ?2 ESCAPE '\\'
                        OR file_path LIKE ?3 ESCAPE '\\'",
                    params![
                        dir_path_text.clone(),
                        like_prefix_slash.clone(),
                        like_prefix_backslash.clone()
                    ],
                )
                .map_err(map_sqlite_error)?;
            tx.execute(
                "UPDATE file_catalog
                 SET removed_at = ?4
                 WHERE file_path = ?1
                    OR file_path LIKE ?2 ESCAPE '\\'
                    OR file_path LIKE ?3 ESCAPE '\\'",
                params![
                    dir_path_text.clone(),
                    like_prefix_slash.clone(),
                    like_prefix_backslash.clone(),
                    current_unix_timestamp_secs()?
                ],
            )
            .map_err(map_sqlite_error)?;
            let removed_doc_rows = tx
                .execute(
                    "DELETE FROM documents
                     WHERE file_path = ?1
                        OR file_path LIKE ?2 ESCAPE '\\'
                        OR file_path LIKE ?3 ESCAPE '\\'",
                    params![dir_path_text, like_prefix_slash, like_prefix_backslash],
                )
                .map_err(map_sqlite_error)?;

            tx.commit().map_err(map_sqlite_error)?;
            (doc_ids, removed_state_rows, removed_doc_rows)
        };

        if !removed_doc_ids.is_empty() {
            let removed_doc_ids: std::collections::HashSet<i64> =
                removed_doc_ids.into_iter().collect();
            let mut cache_guard = self.cache.write().await;
            cache_guard.retain(|item| !removed_doc_ids.contains(&item.doc_id));
        }

        Ok(removed_state_rows > 0 || removed_doc_rows > 0)
    }

    pub async fn enqueue_graph_task(
        &self,
        chunk_id: i64,
        content_hash: &str,
        content: &str,
    ) -> Result<(), StorageError> {
        let now = current_unix_timestamp_secs()?;
        let conn_guard = self.lock_conn()?;
        conn_guard
            .execute(
                "INSERT INTO graph_task_queue(chunk_id, content, content_hash, status, retry_count, updated_at)
                 VALUES(?1, ?2, ?3, 'pending', 0, ?4)
                 ON CONFLICT(chunk_id, content_hash) DO UPDATE SET
                   status = CASE
                     WHEN graph_task_queue.status = 'done' THEN graph_task_queue.status
                     ELSE 'pending'
                   END,
                   content = excluded.content,
                   updated_at = excluded.updated_at",
                params![chunk_id, content, content_hash, now],
            )
            .map_err(map_sqlite_error)?;
        Ok(())
    }

    pub async fn fetch_next_graph_task(&self) -> Result<Option<GraphTaskRecord>, StorageError> {
        let mut conn_guard = self.lock_conn()?;
        let tx = conn_guard.transaction().map_err(map_sqlite_error)?;
        let task = tx
            .query_row(
                "SELECT task_id, chunk_id, content_hash, status, retry_count
                 , content
                 FROM graph_task_queue
                 WHERE status = 'pending'
                 ORDER BY updated_at ASC, task_id ASC
                 LIMIT 1",
                [],
                |row| {
                    Ok(GraphTaskRecord {
                        task_id: row.get(0)?,
                        chunk_id: row.get(1)?,
                        content_hash: row.get(2)?,
                        status: row.get(3)?,
                        retry_count: row.get(4)?,
                        content: row.get(5)?,
                    })
                },
            )
            .optional()
            .map_err(map_sqlite_error)?;

        if let Some(task) = task {
            let now = current_unix_timestamp_secs()?;
            tx.execute(
                "UPDATE graph_task_queue
                 SET status = 'running', updated_at = ?2
                 WHERE task_id = ?1",
                params![task.task_id, now],
            )
            .map_err(map_sqlite_error)?;
            tx.commit().map_err(map_sqlite_error)?;
            return Ok(Some(task));
        }

        tx.commit().map_err(map_sqlite_error)?;
        Ok(None)
    }

    pub async fn mark_graph_task_done(&self, task_id: i64) -> Result<(), StorageError> {
        let now = current_unix_timestamp_secs()?;
        let conn_guard = self.lock_conn()?;
        conn_guard
            .execute(
                "UPDATE graph_task_queue
                 SET status = 'done', updated_at = ?2
                 WHERE task_id = ?1",
                params![task_id, now],
            )
            .map_err(map_sqlite_error)?;
        Ok(())
    }

    pub async fn mark_graph_task_failed(
        &self,
        task_id: i64,
        retry_count: i64,
    ) -> Result<(), StorageError> {
        let now = current_unix_timestamp_secs()?;
        let next_status = if retry_count >= 3 {
            "failed"
        } else {
            "pending"
        };
        let conn_guard = self.lock_conn()?;
        conn_guard
            .execute(
                "UPDATE graph_task_queue
                 SET status = ?2, retry_count = ?3, updated_at = ?4
                 WHERE task_id = ?1",
                params![task_id, next_status, retry_count, now],
            )
            .map_err(map_sqlite_error)?;
        Ok(())
    }

    pub async fn count_graph_backlog(&self) -> Result<u64, StorageError> {
        let conn_guard = self.lock_conn()?;
        let count: i64 = conn_guard
            .query_row(
                "SELECT COUNT(*) FROM graph_task_queue WHERE status IN ('pending','running')",
                [],
                |row| row.get(0),
            )
            .map_err(map_sqlite_error)?;
        u64::try_from(count).map_err(|_| StorageError::NegativeCount {
            table: "graph_task_queue",
            count,
        })
    }

    pub async fn count_graphed_chunks(&self) -> Result<u64, StorageError> {
        let conn_guard = self.lock_conn()?;
        let count: i64 = conn_guard
            .query_row(
                "SELECT COUNT(DISTINCT chunk_id) FROM chunk_nodes",
                [],
                |row| row.get(0),
            )
            .map_err(map_sqlite_error)?;
        u64::try_from(count).map_err(|_| StorageError::NegativeCount {
            table: "chunk_nodes",
            count,
        })
    }

    /// 根据检索到的 chunk_id 列表生成 1-hop 图谱上下文。
    pub async fn get_graph_context_for_chunks(
        &self,
        chunk_ids: &[i64],
    ) -> Result<String, StorageError> {
        if chunk_ids.is_empty() {
            return Ok(String::new());
        }

        let conn_guard = self.lock_conn()?;

        // 1) 先由 chunk_id 找到所有关联节点
        let chunk_placeholders = make_placeholders(chunk_ids.len());
        let node_id_query = format!(
            "SELECT DISTINCT node_id FROM chunk_nodes WHERE chunk_id IN ({})",
            chunk_placeholders
        );
        let mut node_id_stmt = conn_guard
            .prepare(&node_id_query)
            .map_err(map_sqlite_error)?;
        let node_id_rows = node_id_stmt
            .query_map(params_from_iter(chunk_ids.iter()), |row| {
                row.get::<_, String>(0)
            })
            .map_err(map_sqlite_error)?;

        let mut node_ids = Vec::new();
        for row in node_id_rows {
            node_ids.push(row.map_err(map_sqlite_error)?);
        }
        if node_ids.is_empty() {
            return Ok(String::new());
        }

        let mut unique_node_ids = Vec::new();
        let mut seen = HashSet::new();
        for node_id in node_ids {
            if seen.insert(node_id.clone()) {
                unique_node_ids.push(node_id);
            }
        }

        // 2) 加载节点元数据，用于输出可读关系文本
        let node_placeholders = make_placeholders(unique_node_ids.len());
        let node_meta_query = format!(
            "SELECT id, name, label, COALESCE(description, '')
             FROM nodes
             WHERE id IN ({})",
            node_placeholders
        );
        let mut node_meta_stmt = conn_guard
            .prepare(&node_meta_query)
            .map_err(map_sqlite_error)?;
        let node_meta_rows = node_meta_stmt
            .query_map(params_from_iter(unique_node_ids.iter()), |row| {
                let id: String = row.get(0)?;
                let name: String = row.get(1)?;
                let label: String = row.get(2)?;
                let description: String = row.get(3)?;
                Ok((id, name, label, description))
            })
            .map_err(map_sqlite_error)?;

        let mut node_meta = HashMap::new();
        for row in node_meta_rows {
            let (id, name, label, description) = row.map_err(map_sqlite_error)?;
            node_meta.insert(id, (name, label, description));
        }

        // 3) 查询 1-hop 边：source 或 target 命中节点集合即可
        let edge_placeholders = make_placeholders(unique_node_ids.len());
        let edge_query = format!(
            "SELECT id, source_node, target_node, relation
             FROM edges
             WHERE source_node IN ({0}) OR target_node IN ({0})",
            edge_placeholders
        );
        let mut edge_stmt = conn_guard.prepare(&edge_query).map_err(map_sqlite_error)?;
        let edge_params: Vec<&str> = unique_node_ids
            .iter()
            .chain(unique_node_ids.iter())
            .map(String::as_str)
            .collect();
        let edge_rows = edge_stmt
            .query_map(params_from_iter(edge_params), |row| {
                let id: String = row.get(0)?;
                let source_node: String = row.get(1)?;
                let target_node: String = row.get(2)?;
                let relation: String = row.get(3)?;
                Ok((id, source_node, target_node, relation))
            })
            .map_err(map_sqlite_error)?;

        let mut edge_lines = Vec::new();
        for row in edge_rows {
            let (_id, source_node, target_node, relation) = row.map_err(map_sqlite_error)?;

            let source_name = node_meta
                .get(&source_node)
                .map(|(name, _, _)| name.clone())
                .unwrap_or(source_node);
            let target_name = node_meta
                .get(&target_node)
                .map(|(name, _, _)| name.clone())
                .unwrap_or(target_node);

            edge_lines.push(format!(
                "[{}] - ({}) -> [{}]",
                source_name, relation, target_name
            ));
        }
        edge_lines.sort();
        edge_lines.dedup();

        // 如果暂无边，回退到节点摘要，给上层 LLM 一个可用图谱上下文。
        if edge_lines.is_empty() {
            let mut node_lines = Vec::new();
            for node_id in unique_node_ids {
                if let Some((name, label, description)) = node_meta.get(&node_id) {
                    if description.trim().is_empty() {
                        node_lines.push(format!("[{}] ({})", name, label));
                    } else {
                        node_lines.push(format!("[{}] ({}) - {}", name, label, description));
                    }
                }
            }
            node_lines.sort();
            node_lines.dedup();
            return Ok(node_lines.join("\n"));
        }

        Ok(edge_lines.join("\n"))
    }

    fn lock_conn(&self) -> Result<std::sync::MutexGuard<'_, Connection>, StorageError> {
        self.conn
            .lock()
            .map_err(|_| StorageError::LockPoisoned("sqlite connection"))
    }

    /// 统计 documents 表总行数。
    pub async fn count_documents(&self) -> Result<u64, StorageError> {
        let conn_guard = self.lock_conn()?;
        let count: i64 = conn_guard
            .query_row("SELECT COUNT(*) FROM documents", [], |row| row.get(0))
            .map_err(map_sqlite_error)?;
        u64::try_from(count).map_err(|_| StorageError::NegativeCount {
            table: "documents",
            count,
        })
    }

    /// 统计 chunks 表总行数。
    pub async fn count_chunks(&self) -> Result<u64, StorageError> {
        let conn_guard = self.lock_conn()?;
        let count: i64 = conn_guard
            .query_row("SELECT COUNT(*) FROM chunks", [], |row| row.get(0))
            .map_err(map_sqlite_error)?;
        u64::try_from(count).map_err(|_| StorageError::NegativeCount {
            table: "chunks",
            count,
        })
    }

    pub async fn embedding_dimension(&self) -> Result<Option<usize>, StorageError> {
        {
            let cache_guard = self.cache.read().await;
            if let Some(size) = cache_guard.first().map(|item| item.embedding.len()) {
                return Ok(Some(size));
            }
        }

        let conn_guard = self.lock_conn()?;
        let blob = conn_guard
            .query_row(
                "SELECT c.embedding_blob
                 FROM chunks c
                 INNER JOIN documents d ON d.id = c.doc_id
                 INNER JOIN file_catalog fc ON fc.file_path = d.file_path
                 WHERE fc.removed_at IS NULL
                 LIMIT 1",
                [],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()
            .map_err(map_sqlite_error)?;

        let Some(blob) = blob else {
            return Ok(None);
        };
        let embedding: Vec<f32> =
            bincode::deserialize(&blob).map_err(StorageError::DeserializeEmbedding)?;
        Ok(Some(embedding.len()))
    }

    /// 统计 nodes 表总行数。
    pub async fn count_nodes(&self) -> Result<u64, StorageError> {
        let conn_guard = self.lock_conn()?;
        let count: i64 = conn_guard
            .query_row("SELECT COUNT(*) FROM nodes", [], |row| row.get(0))
            .map_err(map_sqlite_error)?;
        u64::try_from(count).map_err(|_| StorageError::NegativeCount {
            table: "nodes",
            count,
        })
    }

    pub async fn search_similar_scoped(
        &self,
        query_embedding: Vec<f32>,
        top_k: usize,
        scope_paths: &[PathBuf],
    ) -> Result<Vec<(DocumentChunk, f32)>, StorageError> {
        if top_k == 0 || query_embedding.is_empty() {
            return Ok(Vec::new());
        }

        let scope_matchers = build_scope_matchers(scope_paths);

        let mut top = {
            let cache_guard = self.cache.read().await;
            let mut scored: Vec<(i64, f32)> = cache_guard
                .iter()
                .filter(|item| matches_scopes(&item.file_path, &scope_matchers))
                .map(|item| {
                    let score = cosine_similarity(&query_embedding, &item.embedding);
                    (item.chunk_id, score)
                })
                .collect();

            scored.sort_by(|a, b| b.1.total_cmp(&a.1));
            scored.truncate(top_k);
            scored
        };

        if top.is_empty() {
            return Ok(Vec::new());
        }

        let conn_guard = self.lock_conn()?;
        let mut stmt = conn_guard
            .prepare(
                "SELECT c.chunk_index, c.content, d.file_path, c.heading_path_json, c.block_kind
                 FROM chunks c
                 INNER JOIN documents d ON d.id = c.doc_id
                 INNER JOIN file_catalog fc ON fc.file_path = d.file_path
                 WHERE c.id = ?1
                   AND fc.removed_at IS NULL",
            )
            .map_err(map_sqlite_error)?;

        let mut results = Vec::with_capacity(top.len());
        for (chunk_id, score) in top.drain(..) {
            let row = stmt
                .query_row(params![chunk_id], |row| {
                    let chunk_index_raw: i64 = row.get(0)?;
                    let content: String = row.get(1)?;
                    let file_path: String = row.get(2)?;
                    let heading_path_json: String = row.get(3)?;
                    let block_kind: String = row.get(4)?;
                    Ok((
                        chunk_index_raw,
                        content,
                        file_path,
                        heading_path_json,
                        block_kind,
                    ))
                })
                .optional()
                .map_err(map_sqlite_error)?;

            if let Some((chunk_index_raw, content, file_path_text, heading_path_json, block_kind)) =
                row
            {
                let chunk_index = usize::try_from(chunk_index_raw).map_err(|_| {
                    StorageError::InvalidChunkIndex {
                        raw: chunk_index_raw,
                    }
                })?;
                let heading_path = parse_heading_path_json(&heading_path_json)?;

                results.push((
                    DocumentChunk {
                        file_path: PathBuf::from(file_path_text),
                        content,
                        chunk_index,
                        heading_path,
                        block_kind: chunk_block_kind_from_storage(&block_kind),
                    },
                    score,
                ));
            }
        }

        Ok(results)
    }
}

impl VectorStore for SqliteStore {
    async fn insert_chunks(
        &self,
        chunks: Vec<DocumentChunk>,
        embeddings: Vec<Vec<f32>>,
    ) -> Result<(), StorageError> {
        if chunks.is_empty() {
            return Err(StorageError::EmptyChunks);
        }
        let file_path = chunks[0].file_path.clone();
        if chunks.iter().any(|chunk| chunk.file_path != file_path) {
            return Err(StorageError::MixedFilePathInBatch);
        }
        self.replace_document_index(
            &file_path,
            None,
            current_unix_timestamp_secs()?,
            "",
            chunks,
            embeddings,
        )
        .await
    }

    async fn search_similar(
        &self,
        query_embedding: Vec<f32>,
        top_k: usize,
    ) -> Result<Vec<(DocumentChunk, f32)>, StorageError> {
        self.search_similar_scoped(query_embedding, top_k, &[])
            .await
    }
}

#[derive(Debug, Clone)]
enum ScopeMatcher {
    File(String),
    Dir(String),
}

fn build_scope_matchers(scope_paths: &[PathBuf]) -> Vec<ScopeMatcher> {
    let mut matchers = Vec::new();
    for scope in scope_paths {
        let trimmed = scope.to_string_lossy().trim().to_string();
        if trimmed.is_empty() {
            continue;
        }
        let path = PathBuf::from(trimmed);
        let normalized = normalize_scope_path_text(&path);
        if normalized.is_empty() {
            continue;
        }
        match std::fs::metadata(&path) {
            Ok(meta) if meta.is_file() => matchers.push(ScopeMatcher::File(normalized)),
            _ => matchers.push(ScopeMatcher::Dir(normalized)),
        }
    }
    matchers
}

fn matches_scopes(file_path: &Path, scope_matchers: &[ScopeMatcher]) -> bool {
    if scope_matchers.is_empty() {
        return true;
    }

    let normalized_file = normalize_scope_path_text(file_path);
    if normalized_file.is_empty() {
        return false;
    }

    scope_matchers.iter().any(|matcher| match matcher {
        ScopeMatcher::File(scope_file) => normalized_file == *scope_file,
        ScopeMatcher::Dir(scope_dir) => path_is_within_scope_dir(&normalized_file, scope_dir),
    })
}

fn normalize_scope_path_text(path: &Path) -> String {
    #[cfg(target_os = "windows")]
    {
        let mut text = path.to_string_lossy().replace('/', "\\");

        if let Some(stripped) = text.strip_prefix(r"\\?\") {
            text = stripped.to_string();
        } else if let Some(stripped) = text.strip_prefix(r"\??\") {
            text = stripped.to_string();
        }

        while text.len() > 3 && text.ends_with('\\') {
            text.pop();
        }

        text.to_ascii_lowercase()
    }

    #[cfg(not(target_os = "windows"))]
    {
        let mut text = path.to_string_lossy().to_string();
        while text.len() > 1 && text.ends_with('/') {
            text.pop();
        }
        text
    }
}

fn path_is_within_scope_dir(file_path: &str, scope_dir: &str) -> bool {
    if file_path == scope_dir {
        return true;
    }

    if scope_dir.is_empty() {
        return false;
    }

    #[cfg(target_os = "windows")]
    let sep = '\\';
    #[cfg(not(target_os = "windows"))]
    let sep = '/';

    let mut prefix = scope_dir.to_string();
    if !prefix.ends_with(sep) {
        prefix.push(sep);
    }

    file_path.starts_with(&prefix)
}

fn normalize_storage_file_path_text(path: &Path) -> String {
    normalize_scope_path_text(path)
}

fn build_relative_path(file_path: &Path, watch_root: Option<&Path>) -> String {
    if let Some(root) = watch_root {
        if let Ok(relative) = file_path.strip_prefix(root) {
            let text = relative.to_string_lossy().to_string();
            if !text.trim().is_empty() {
                return normalize_relative_path_text(&text);
            }
        }
    }

    file_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(normalize_relative_path_text)
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| normalize_relative_path_text(&file_path.to_string_lossy()))
}

fn normalize_relative_path_text(text: impl AsRef<str>) -> String {
    #[cfg(target_os = "windows")]
    {
        text.as_ref().replace('\\', "/")
    }

    #[cfg(not(target_os = "windows"))]
    {
        text.as_ref().to_string()
    }
}

fn inferred_file_size(chunks: &[DocumentChunk]) -> i64 {
    let byte_len: usize = chunks.iter().map(|chunk| chunk.content.len()).sum();
    i64::try_from(byte_len).unwrap_or(i64::MAX)
}

fn build_heading_catalog_text(chunks: &[DocumentChunk]) -> String {
    let mut seen = HashSet::new();
    let mut headings = Vec::new();

    for chunk in chunks {
        if chunk.heading_path.is_empty() {
            continue;
        }
        let joined = chunk.heading_path.join(" / ");
        if seen.insert(joined.clone()) {
            headings.push(joined);
        }
    }

    headings.join("\n")
}

fn build_document_search_text(
    catalog_entry: &CatalogEntry,
    heading_catalog_text: &str,
    chunks: &[DocumentChunk],
) -> String {
    let mut sections = Vec::new();

    if !catalog_entry.file_name.trim().is_empty() {
        sections.push(catalog_entry.file_name.trim().to_string());
    }
    if !catalog_entry.relative_path.trim().is_empty()
        && catalog_entry.relative_path != catalog_entry.file_name
    {
        sections.push(catalog_entry.relative_path.trim().to_string());
    }
    if !heading_catalog_text.trim().is_empty() {
        sections.push(heading_catalog_text.trim().to_string());
    }

    let code_symbol_text = build_code_symbol_text(catalog_entry, chunks);
    if !code_symbol_text.trim().is_empty() {
        sections.push(code_symbol_text);
    }

    let preview = build_document_preview_text(chunks);
    if !preview.trim().is_empty() {
        sections.push(preview.trim().to_string());
    }

    sections.join("\n")
}

fn build_document_preview_text(chunks: &[DocumentChunk]) -> String {
    let mut snippets = Vec::new();
    let mut seen_snippets = HashSet::new();
    for chunk in chunks {
        let snippet = chunk_preview_snippet(&chunk.content);
        if snippet.is_empty() || !seen_snippets.insert(snippet.clone()) {
            continue;
        }
        snippets.push(snippet);
    }

    if snippets.is_empty() {
        return String::new();
    }

    let sample_count = snippets.len().min(DOCUMENT_SEARCH_MAX_SNIPPETS);
    let sampled_indices = evenly_sample_indices(snippets.len(), sample_count);
    let mut preview = String::new();

    for index in sampled_indices {
        let Some(snippet) = snippets.get(index) else {
            continue;
        };
        let candidate_len =
            preview.chars().count() + snippet.chars().count() + usize::from(!preview.is_empty());
        if candidate_len > DOCUMENT_SEARCH_PREVIEW_CHARS {
            break;
        }

        if !preview.is_empty() {
            preview.push('\n');
        }
        preview.push_str(snippet);
    }

    preview
}

fn evenly_sample_indices(total: usize, count: usize) -> Vec<usize> {
    if total == 0 || count == 0 {
        return Vec::new();
    }
    if count >= total {
        return (0..total).collect();
    }

    let max_index = total - 1;
    let denominator = count - 1;
    let mut indices = Vec::with_capacity(count);
    for position in 0..count {
        let index = if denominator == 0 {
            0
        } else {
            (position * max_index + denominator / 2) / denominator
        };
        if indices.last().copied() != Some(index) {
            indices.push(index);
        }
    }
    indices
}

fn build_code_symbol_text(catalog_entry: &CatalogEntry, chunks: &[DocumentChunk]) -> String {
    if !is_code_like_document(catalog_entry) {
        return String::new();
    }

    let mut symbols = Vec::new();
    let mut seen = HashSet::new();
    for chunk in chunks {
        extract_code_symbol_terms(&chunk.content, &mut symbols, &mut seen);
    }

    let mut rendered = String::new();
    for symbol in symbols {
        let candidate_len =
            rendered.chars().count() + symbol.chars().count() + usize::from(!rendered.is_empty());
        if candidate_len > DOCUMENT_SEARCH_SYMBOL_CHARS {
            break;
        }
        if !rendered.is_empty() {
            rendered.push('\n');
        }
        rendered.push_str(&symbol);
    }

    rendered
}

fn is_code_like_document(catalog_entry: &CatalogEntry) -> bool {
    matches!(
        catalog_entry
            .file_ext
            .trim()
            .trim_start_matches('.')
            .to_ascii_lowercase()
            .as_str(),
        "rs" | "ts" | "tsx" | "js" | "jsx"
    )
}

fn extract_code_symbol_terms(content: &str, symbols: &mut Vec<String>, seen: &mut HashSet<String>) {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }

        for keyword in ["fn", "struct", "enum", "type", "trait", "impl", "mod"] {
            if let Some(identifier) = extract_identifier_after_keyword(trimmed, keyword) {
                push_unique_symbol(symbols, seen, &identifier);
            }
        }

        if let Some(identifier) = extract_field_name(trimmed) {
            push_unique_symbol(symbols, seen, &identifier);
        }

        for qualified in extract_qualified_identifiers(trimmed) {
            push_unique_symbol(symbols, seen, &qualified);
        }

        for literal in extract_interesting_string_literals(trimmed) {
            push_unique_symbol(symbols, seen, &literal);
        }
    }
}

fn extract_identifier_after_keyword(line: &str, keyword: &str) -> Option<String> {
    let bytes = line.as_bytes();
    let keyword_bytes = keyword.as_bytes();
    let mut index = 0;

    while index + keyword_bytes.len() <= bytes.len() {
        if &bytes[index..index + keyword_bytes.len()] == keyword_bytes {
            let before = index
                .checked_sub(1)
                .and_then(|pos| bytes.get(pos))
                .copied()
                .map(char::from);
            let after = bytes
                .get(index + keyword_bytes.len())
                .copied()
                .map(char::from);
            let before_is_ident = before.is_some_and(is_identifier_char);
            let after_is_boundary = after.is_none_or(|ch| ch.is_whitespace() || ch == '<');
            if !before_is_ident && after_is_boundary {
                let rest = line[index + keyword_bytes.len()..].trim_start();
                let identifier = take_identifier(rest);
                if is_symbol_identifier(&identifier) {
                    return Some(identifier);
                }
            }
        }
        index += 1;
    }

    None
}

fn extract_field_name(line: &str) -> Option<String> {
    let mut rest = line.trim_start();
    for prefix in ["pub(crate) ", "pub(super) ", "pub(self) ", "pub "] {
        if let Some(stripped) = rest.strip_prefix(prefix) {
            rest = stripped.trim_start();
            break;
        }
    }

    let identifier = take_identifier(rest);
    if !is_symbol_identifier(&identifier) {
        return None;
    }

    let remaining = rest[identifier.len()..].trim_start();
    if remaining.starts_with(':') {
        Some(identifier)
    } else {
        None
    }
}

fn extract_interesting_string_literals(line: &str) -> Vec<String> {
    let mut literals = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut escaped = false;

    for ch in line.chars() {
        if in_string {
            if escaped {
                current.push(ch);
                escaped = false;
                continue;
            }

            match ch {
                '\\' => escaped = true,
                '"' => {
                    if is_interesting_symbol_literal(&current) {
                        literals.push(current.trim().to_string());
                    }
                    current.clear();
                    in_string = false;
                }
                _ => current.push(ch),
            }
        } else if ch == '"' {
            in_string = true;
        }
    }

    literals
}

fn extract_qualified_identifiers(line: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut current = String::new();

    for ch in line.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | ':') {
            current.push(ch);
            continue;
        }

        maybe_push_qualified_identifier(&mut values, &mut current);
    }
    maybe_push_qualified_identifier(&mut values, &mut current);

    values
}

fn maybe_push_qualified_identifier(target: &mut Vec<String>, current: &mut String) {
    let candidate = current
        .trim_matches(|ch: char| matches!(ch, '.' | ':'))
        .to_string();
    current.clear();

    if candidate.is_empty() || (!candidate.contains('.') && !candidate.contains("::")) {
        return;
    }

    let normalized = candidate.replace("::", ".");
    let valid = normalized
        .split('.')
        .filter(|segment| !segment.is_empty())
        .all(is_symbol_identifier);
    if valid && normalized.chars().count() >= 8 {
        target.push(normalized);
    }
}

fn is_interesting_symbol_literal(literal: &str) -> bool {
    let trimmed = literal.trim();
    if trimmed.is_empty() {
        return false;
    }

    trimmed.contains('/')
        || trimmed.contains('_')
        || trimmed.contains('.')
        || trimmed.contains('-')
        || trimmed.contains("::")
        || (trimmed.chars().count() >= 6 && trimmed.chars().any(|ch| ch.is_ascii_alphabetic()))
}

fn take_identifier(text: &str) -> String {
    text.chars()
        .take_while(|ch| is_identifier_char(*ch))
        .collect::<String>()
}

fn is_identifier_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn is_symbol_identifier(identifier: &str) -> bool {
    let trimmed = identifier.trim();
    !trimmed.is_empty()
        && trimmed
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
}

fn chunk_preview_snippet(content: &str) -> String {
    let collapsed = content
        .split_whitespace()
        .filter(|segment| !segment.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let trimmed = collapsed.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    trimmed
        .chars()
        .take(DOCUMENT_SEARCH_SNIPPET_CHARS)
        .collect::<String>()
        .trim()
        .to_string()
}

fn parse_heading_path_json(raw: &str) -> Result<Vec<String>, StorageError> {
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(raw).map_err(StorageError::SerdeJson)
}

fn chunk_block_kind_to_storage(kind: ChunkBlockKind) -> &'static str {
    match kind {
        ChunkBlockKind::Heading => "heading",
        ChunkBlockKind::Paragraph => "paragraph",
        ChunkBlockKind::List => "list",
        ChunkBlockKind::CodeBlock => "code_block",
        ChunkBlockKind::Table => "table",
        ChunkBlockKind::Quote => "quote",
        ChunkBlockKind::Html => "html",
        ChunkBlockKind::ThematicBreak => "thematic_break",
        ChunkBlockKind::Mixed => "mixed",
    }
}

fn chunk_block_kind_from_storage(raw: &str) -> ChunkBlockKind {
    match raw.trim() {
        "heading" => ChunkBlockKind::Heading,
        "list" => ChunkBlockKind::List,
        "code_block" => ChunkBlockKind::CodeBlock,
        "table" => ChunkBlockKind::Table,
        "quote" => ChunkBlockKind::Quote,
        "html" => ChunkBlockKind::Html,
        "thematic_break" => ChunkBlockKind::ThematicBreak,
        "mixed" => ChunkBlockKind::Mixed,
        _ => ChunkBlockKind::Paragraph,
    }
}

fn map_document_record(row: &rusqlite::Row<'_>) -> Result<DocumentRecord, rusqlite::Error> {
    let chunk_count_raw: i64 = row.get(7)?;
    let content_char_count_raw: i64 = row.get(8)?;

    Ok(DocumentRecord {
        id: row.get(0)?,
        file_path: row.get(1)?,
        relative_path: row.get(2)?,
        file_name: row.get(3)?,
        file_ext: row.get(4)?,
        last_modified: row.get(5)?,
        indexed_at: row.get(6)?,
        chunk_count: u32::try_from(chunk_count_raw).unwrap_or_default(),
        content_char_count: u32::try_from(content_char_count_raw).unwrap_or_default(),
        heading_catalog_text: row.get(9)?,
        document_search_text: row.get(10)?,
    })
}

fn map_chunk_record(row: &rusqlite::Row<'_>) -> Result<ChunkRecord, rusqlite::Error> {
    let chunk_index_raw: i64 = row.get(2)?;
    let heading_path_json: String = row.get(4)?;
    let char_len_raw: i64 = row.get(6)?;
    let heading_path = serde_json::from_str(&heading_path_json).unwrap_or_default();

    Ok(ChunkRecord {
        id: row.get(0)?,
        doc_id: row.get(1)?,
        chunk_index: usize::try_from(chunk_index_raw).unwrap_or_default(),
        content: row.get(3)?,
        heading_path,
        block_kind: row.get(5)?,
        char_len: u32::try_from(char_len_raw).unwrap_or_default(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FtsQueryMode {
    BroadOr,
    StrictAnd,
}

fn build_fts_match_query(query: &str, mode: FtsQueryMode) -> Option<String> {
    let mut terms = Vec::new();
    let mut seen = HashSet::new();

    for term in extract_fts_terms(query) {
        if seen.insert(term.clone()) {
            terms.push(format!("\"{}\"", term.replace('"', "\"\"")));
        }
    }

    if terms.is_empty() {
        None
    } else {
        Some(match mode {
            FtsQueryMode::BroadOr => terms.join(" OR "),
            FtsQueryMode::StrictAnd => terms.join(" AND "),
        })
    }
}

fn extract_fts_terms(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut seen = HashSet::new();

    for token in extract_query_tokens(query) {
        for term in expand_query_token(&token) {
            let normalized = term.trim().to_string();
            if is_viable_search_term(&normalized) && seen.insert(normalized.clone()) {
                terms.push(normalized);
            }
        }
    }

    terms
}

fn extract_signal_terms(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut seen = HashSet::new();
    for token in extract_query_tokens(query) {
        for term in expand_query_token(&token) {
            let normalized = normalize_ascii_term(&term);
            if is_viable_signal_term(&normalized) && seen.insert(normalized.clone()) {
                terms.push(normalized);
            }
        }
    }
    terms
}

fn extract_phrase_signal_terms(query: &str) -> Vec<String> {
    let filtered_tokens = extract_query_tokens(query)
        .into_iter()
        .map(|token| normalize_ascii_term(&token))
        .filter(|token| is_viable_search_term(token) && !is_english_stopword(token))
        .collect::<Vec<_>>();

    let mut phrases = Vec::new();
    let mut seen = HashSet::new();
    let max_window = filtered_tokens.len().min(4);

    if filtered_tokens.len() == 1 {
        let token = filtered_tokens[0].clone();
        if is_viable_phrase_signal_term(&token) && seen.insert(token.clone()) {
            phrases.push(token);
        }
    }

    for window in (2..=max_window).rev() {
        for slice in filtered_tokens.windows(window) {
            let phrase = slice.join(" ");
            if is_viable_phrase_signal_term(&phrase) && seen.insert(phrase.clone()) {
                phrases.push(phrase);
            }
        }
    }

    if filtered_tokens.len() <= 6 {
        let full_phrase = filtered_tokens.join(" ");
        if is_viable_phrase_signal_term(&full_phrase) && seen.insert(full_phrase.clone()) {
            phrases.push(full_phrase);
        }
    }

    phrases
}

fn extract_query_tokens(query: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();

    for ch in query.chars() {
        if is_query_term_char(ch) {
            current.push(ch);
            continue;
        }

        if !current.is_empty() {
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

    let mut results = Vec::new();
    let normalized = normalize_ascii_term(trimmed);
    if !normalized.is_empty() {
        results.push(normalized.clone());
    }

    if trimmed.chars().any(is_cjk_char) {
        let cjk_only = trimmed
            .chars()
            .filter(|ch| is_cjk_char(*ch) || ch.is_ascii_digit())
            .collect::<String>();
        if !cjk_only.is_empty() {
            results.push(cjk_only.clone());
        }
        let pure_cjk = cjk_only
            .chars()
            .filter(|ch| is_cjk_char(*ch))
            .collect::<String>();
        if !pure_cjk.is_empty() {
            results.push(pure_cjk.clone());
        }
        for phrase in extract_cjk_phrases(&cjk_only) {
            results.push(phrase);
        }
        for phrase in extract_cjk_phrases(&pure_cjk) {
            results.push(phrase);
        }
    }

    if trimmed
        .chars()
        .any(|ch| matches!(ch, '_' | '-' | '.' | '/' | '\\'))
    {
        if let Some((stem, _ext)) = trimmed.rsplit_once('.') {
            let stem = normalize_ascii_term(stem);
            if !stem.is_empty() {
                results.push(stem);
            }
        }
        for segment in trimmed.split(['_', '-', '.', '/', '\\']) {
            let segment = normalize_ascii_term(segment);
            if !segment.is_empty() {
                results.push(segment);
            }
        }
    }

    let mut deduped = Vec::new();
    let mut seen = HashSet::new();
    for item in results {
        if seen.insert(item.clone()) {
            deduped.push(item);
        }
    }
    deduped
}

fn extract_cjk_phrases(token: &str) -> Vec<String> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let mut phrases = Vec::new();
    let full = trimmed.to_string();
    if !is_cjk_question_phrase(&full) {
        phrases.push(full.clone());
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
        if full.ends_with(suffix) {
            let candidate = full.trim_end_matches(suffix).trim().to_string();
            if candidate.chars().count() >= 2 {
                phrases.push(candidate);
            }
        }
    }

    let mut deduped = Vec::new();
    let mut seen = HashSet::new();
    for phrase in phrases {
        if is_viable_search_term(&phrase) && seen.insert(phrase.clone()) {
            deduped.push(phrase);
        }
    }
    deduped
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
            | "哪个"
            | "哪一个"
            | "哪里"
            | "在哪"
            | "谁"
    )
}

fn normalize_ascii_term(term: &str) -> String {
    term.trim()
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

fn is_viable_search_term(term: &str) -> bool {
    let trimmed = term.trim();
    if trimmed.is_empty() {
        return false;
    }
    if is_english_stopword(trimmed) {
        return false;
    }
    if trimmed.chars().any(is_cjk_char) {
        return trimmed.chars().count() >= 2;
    }
    if trimmed.chars().all(|ch| ch.is_ascii_digit()) {
        return trimmed.chars().count() >= 1;
    }
    trimmed.chars().count() >= 2
}

fn is_viable_signal_term(term: &str) -> bool {
    let trimmed = term.trim();
    if trimmed.is_empty() {
        return false;
    }
    if is_english_stopword(trimmed) {
        return false;
    }
    if trimmed.chars().any(is_cjk_char) {
        return trimmed.chars().count() >= 2;
    }
    trimmed.chars().count() >= 2 || trimmed.chars().all(|ch| ch.is_ascii_digit())
}

fn is_viable_phrase_signal_term(term: &str) -> bool {
    let trimmed = term.trim();
    !trimmed.is_empty()
        && (trimmed.chars().count() >= 6
            || trimmed.chars().any(is_cjk_char)
            || trimmed
                .chars()
                .any(|ch| matches!(ch, '_' | '-' | '.' | '/' | '\\')))
}

fn is_query_term_char(ch: char) -> bool {
    is_cjk_char(ch) || ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/' | '\\')
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

fn score_document_signal_match(
    signal_terms: &[String],
    file_name: &str,
    relative_path: &str,
    heading_catalog_text: &str,
    document_search_text: &str,
) -> Option<(i64, Vec<String>)> {
    let file_name_lower = file_name.to_ascii_lowercase();
    let relative_path_lower = relative_path.to_ascii_lowercase();
    let heading_lower = heading_catalog_text.to_ascii_lowercase();
    let document_search_lower = document_search_text.to_ascii_lowercase();
    let is_code_file = is_code_like_path(relative_path, file_name);

    let mut score = 0_i64;
    let mut matched_fields = Vec::new();

    for term in signal_terms {
        let term_lower = term.to_ascii_lowercase();
        if term_lower.is_empty() {
            continue;
        }

        if file_name_lower == term_lower || relative_path_lower == term_lower {
            score += 160;
            push_unique_field(&mut matched_fields, "exact_path");
            continue;
        }

        if file_name_lower.starts_with(&term_lower) {
            score += 80;
            push_unique_field(&mut matched_fields, "file_name");
        } else if file_name_lower.contains(&term_lower) {
            score += 55;
            push_unique_field(&mut matched_fields, "file_name");
        }

        if relative_path_lower.starts_with(&term_lower) {
            score += 65;
            push_unique_field(&mut matched_fields, "relative_path");
        } else if relative_path_lower.contains(&term_lower) {
            score += 45;
            push_unique_field(&mut matched_fields, "relative_path");
        }

        if heading_lower.contains(&term_lower) {
            score += 20;
            push_unique_field(&mut matched_fields, "heading_catalog");
        }

        if is_specific_document_signal_term(&term_lower)
            && document_search_lower.contains(&term_lower)
        {
            if is_code_file && looks_like_code_symbol_term(&term_lower) {
                score += 95;
                push_unique_field(&mut matched_fields, "exact_symbol");
            } else {
                score += if term_lower.chars().any(is_cjk_char)
                    || term_lower.chars().any(|ch| ch.is_ascii_digit())
                    || term_lower
                        .chars()
                        .any(|ch| matches!(ch, '.' | '/' | '\\' | '_' | '-'))
                {
                    40
                } else {
                    18
                };
                push_unique_field(&mut matched_fields, "document_search_text");
            }
        }
    }

    if score == 0 {
        None
    } else {
        Some((score, matched_fields))
    }
}

fn score_document_phrase_signal_match(
    phrase_terms: &[String],
    file_name: &str,
    relative_path: &str,
    heading_catalog_text: &str,
    document_search_text: &str,
) -> Option<(i64, Vec<String>)> {
    let file_name_lower = file_name.to_ascii_lowercase();
    let relative_path_lower = relative_path.to_ascii_lowercase();
    let heading_lower = heading_catalog_text.to_ascii_lowercase();
    let document_search_lower = document_search_text.to_ascii_lowercase();

    let mut score = 0_i64;
    let mut matched_fields = Vec::new();

    for phrase in phrase_terms {
        let needle = phrase.trim().to_ascii_lowercase();
        if needle.is_empty() {
            continue;
        }

        if file_name_lower.contains(&needle) || relative_path_lower.contains(&needle) {
            score += 90;
            push_unique_field(&mut matched_fields, "docs_phrase");
        }
        if heading_lower.contains(&needle) {
            score += 120;
            push_unique_field(&mut matched_fields, "docs_phrase");
        }
        if document_search_lower.contains(&needle) {
            score += if needle.contains('/') || needle.contains('-') || needle.chars().any(is_cjk_char)
            {
                120
            } else {
                100
            };
            push_unique_field(&mut matched_fields, "docs_phrase");
        }
    }

    if score == 0 {
        None
    } else {
        Some((score, matched_fields))
    }
}

fn is_code_like_path(relative_path: &str, file_name: &str) -> bool {
    [relative_path, file_name].iter().any(|value| {
        value
            .rsplit_once('.')
            .map(|(_, ext)| {
                matches!(
                    ext.to_ascii_lowercase().as_str(),
                    "rs" | "ts" | "tsx" | "js" | "jsx"
                )
            })
            .unwrap_or(false)
    })
}

fn looks_like_code_symbol_term(term: &str) -> bool {
    term.chars()
        .any(|ch| matches!(ch, '_' | '/' | '\\' | '.' | '-'))
        || term.chars().any(|ch| ch.is_ascii_digit())
        || term.ends_with("_ms")
}

fn is_specific_document_signal_term(term: &str) -> bool {
    term.chars().any(is_cjk_char)
        || term.chars().any(|ch| ch.is_ascii_digit())
        || term
            .chars()
            .any(|ch| matches!(ch, '.' | '/' | '\\' | '_' | '-'))
        || term.chars().count() >= 4
}

fn push_unique_field(fields: &mut Vec<String>, field: &str) {
    if !fields.iter().any(|item| item == field) {
        fields.push(field.to_string());
    }
}

fn push_unique_symbol(symbols: &mut Vec<String>, seen: &mut HashSet<String>, value: &str) {
    let normalized = value.trim();
    if normalized.is_empty() {
        return;
    }
    if seen.insert(normalized.to_string()) {
        symbols.push(normalized.to_string());
    }
}

fn is_cjk_char(ch: char) -> bool {
    matches!(
        ch as u32,
        0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xF900..=0xFAFF
            | 0x20000..=0x2A6DF
            | 0x2A700..=0x2B73F
            | 0x2B740..=0x2B81F
            | 0x2B820..=0x2CEAF
            | 0x2CEB0..=0x2EBEF
    )
}

fn initialize_schema(conn: &Connection) -> Result<(), StorageError> {
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         CREATE TABLE IF NOT EXISTS documents (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             file_path TEXT NOT NULL UNIQUE,
             relative_path TEXT NOT NULL DEFAULT '',
             file_name TEXT NOT NULL DEFAULT '',
             file_ext TEXT NOT NULL DEFAULT '',
             last_modified INTEGER NOT NULL,
             indexed_at INTEGER NOT NULL DEFAULT 0,
             chunk_count INTEGER NOT NULL DEFAULT 0,
             content_char_count INTEGER NOT NULL DEFAULT 0,
             heading_catalog_text TEXT NOT NULL DEFAULT '',
             document_search_text TEXT NOT NULL DEFAULT ''
         );
         CREATE TABLE IF NOT EXISTS chunks (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             doc_id INTEGER NOT NULL,
             chunk_index INTEGER NOT NULL,
             content TEXT NOT NULL,
             embedding_blob BLOB NOT NULL,
             heading_path_json TEXT NOT NULL DEFAULT '[]',
             block_kind TEXT NOT NULL DEFAULT 'paragraph',
             char_len INTEGER NOT NULL DEFAULT 0,
             FOREIGN KEY(doc_id) REFERENCES documents(id) ON DELETE CASCADE
         );
         CREATE TABLE IF NOT EXISTS nodes (
             id TEXT PRIMARY KEY,
             label TEXT NOT NULL,
             name TEXT NOT NULL,
             description TEXT
         );
         CREATE TABLE IF NOT EXISTS edges (
             id TEXT PRIMARY KEY,
             source_node TEXT NOT NULL,
             target_node TEXT NOT NULL,
             relation TEXT NOT NULL,
             FOREIGN KEY(source_node) REFERENCES nodes(id) ON DELETE CASCADE,
             FOREIGN KEY(target_node) REFERENCES nodes(id) ON DELETE CASCADE
         );
         CREATE TABLE IF NOT EXISTS chunk_nodes (
             chunk_id INTEGER NOT NULL,
             node_id TEXT NOT NULL,
             PRIMARY KEY(chunk_id, node_id),
             FOREIGN KEY(chunk_id) REFERENCES chunks(id) ON DELETE CASCADE,
             FOREIGN KEY(node_id) REFERENCES nodes(id) ON DELETE CASCADE
         );
         CREATE TABLE IF NOT EXISTS file_index_state (
             file_path TEXT PRIMARY KEY,
             file_size INTEGER NOT NULL,
             mtime_secs INTEGER NOT NULL,
             content_hash TEXT NOT NULL,
             indexed_at INTEGER NOT NULL,
             index_status TEXT NOT NULL DEFAULT 'ready',
             last_error TEXT,
             parser_format_version INTEGER NOT NULL DEFAULT 0,
             index_format_version INTEGER NOT NULL DEFAULT 0
         );
         CREATE TABLE IF NOT EXISTS file_catalog (
             file_path TEXT PRIMARY KEY,
             relative_path TEXT NOT NULL,
             parent_dir TEXT NOT NULL DEFAULT '',
             file_name TEXT NOT NULL,
             file_ext TEXT NOT NULL,
             file_size INTEGER NOT NULL,
             mtime_secs INTEGER NOT NULL,
             discovered_at INTEGER NOT NULL,
             removed_at INTEGER
         );
         CREATE TABLE IF NOT EXISTS system_metadata (
             key TEXT PRIMARY KEY,
             value TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS graph_task_queue (
             task_id INTEGER PRIMARY KEY AUTOINCREMENT,
             chunk_id INTEGER NOT NULL,
             content TEXT NOT NULL,
             content_hash TEXT NOT NULL,
             status TEXT NOT NULL,
             retry_count INTEGER NOT NULL DEFAULT 0,
             updated_at INTEGER NOT NULL,
             UNIQUE(chunk_id, content_hash),
             FOREIGN KEY(chunk_id) REFERENCES chunks(id) ON DELETE CASCADE
         );
         CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
             content,
             heading_text,
             file_name,
             relative_path,
             chunk_id UNINDEXED,
             doc_id UNINDEXED,
             file_path UNINDEXED
         );
         CREATE VIRTUAL TABLE IF NOT EXISTS documents_fts USING fts5(
             search_text,
             file_name,
             relative_path,
             heading_catalog_text,
             doc_id UNINDEXED,
             file_path UNINDEXED
         );
         CREATE INDEX IF NOT EXISTS idx_chunks_doc_id ON chunks(doc_id);
         CREATE UNIQUE INDEX IF NOT EXISTS idx_chunks_doc_chunk_index ON chunks(doc_id, chunk_index);
         CREATE INDEX IF NOT EXISTS idx_documents_file_path ON documents(file_path);
         CREATE INDEX IF NOT EXISTS idx_edges_source ON edges(source_node);
         CREATE INDEX IF NOT EXISTS idx_edges_target ON edges(target_node);
         CREATE INDEX IF NOT EXISTS idx_chunk_nodes_chunk_id ON chunk_nodes(chunk_id);
         CREATE INDEX IF NOT EXISTS idx_chunk_nodes_node_id ON chunk_nodes(node_id);
         CREATE INDEX IF NOT EXISTS idx_graph_task_queue_status ON graph_task_queue(status, updated_at);
         CREATE INDEX IF NOT EXISTS idx_file_index_state_indexed_at ON file_index_state(indexed_at);",
    )
    .map_err(map_sqlite_error)?;
    ensure_graph_task_queue_schema(conn)?;
    ensure_documents_schema(conn)?;
    ensure_chunks_schema(conn)?;
    ensure_file_index_state_schema(conn)?;
    ensure_file_catalog_schema(conn)?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_documents_relative_path ON documents(relative_path)",
        [],
    )
    .map_err(map_sqlite_error)?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_file_catalog_removed_at ON file_catalog(removed_at)",
        [],
    )
    .map_err(map_sqlite_error)?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_file_catalog_relative_path ON file_catalog(relative_path)",
        [],
    )
    .map_err(map_sqlite_error)?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_file_catalog_file_name ON file_catalog(file_name)",
        [],
    )
    .map_err(map_sqlite_error)?;
    ensure_schema_version(conn)?;
    ensure_system_metadata(conn)?;
    Ok(())
}

fn ensure_schema_version(conn: &Connection) -> Result<(), StorageError> {
    let current: u32 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(map_sqlite_error)?;
    if current < DB_SCHEMA_VERSION {
        conn.pragma_update(None, "user_version", DB_SCHEMA_VERSION)
            .map_err(map_sqlite_error)?;
    }
    Ok(())
}

fn ensure_graph_task_queue_schema(conn: &Connection) -> Result<(), StorageError> {
    let mut has_content = false;
    let mut stmt = conn
        .prepare("PRAGMA table_info(graph_task_queue)")
        .map_err(map_sqlite_error)?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(map_sqlite_error)?;
    for col in rows {
        if col.map_err(map_sqlite_error)? == "content" {
            has_content = true;
            break;
        }
    }
    if !has_content {
        conn.execute(
            "ALTER TABLE graph_task_queue ADD COLUMN content TEXT NOT NULL DEFAULT ''",
            [],
        )
        .map_err(map_sqlite_error)?;
    }
    Ok(())
}

fn ensure_documents_schema(conn: &Connection) -> Result<(), StorageError> {
    ensure_table_column(
        conn,
        "documents",
        "relative_path",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_table_column(conn, "documents", "file_name", "TEXT NOT NULL DEFAULT ''")?;
    ensure_table_column(conn, "documents", "file_ext", "TEXT NOT NULL DEFAULT ''")?;
    ensure_table_column(
        conn,
        "documents",
        "indexed_at",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_table_column(
        conn,
        "documents",
        "chunk_count",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_table_column(
        conn,
        "documents",
        "content_char_count",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_table_column(
        conn,
        "documents",
        "heading_catalog_text",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_table_column(
        conn,
        "documents",
        "document_search_text",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    Ok(())
}

fn ensure_chunks_schema(conn: &Connection) -> Result<(), StorageError> {
    ensure_table_column(
        conn,
        "chunks",
        "heading_path_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )?;
    ensure_table_column(
        conn,
        "chunks",
        "block_kind",
        "TEXT NOT NULL DEFAULT 'paragraph'",
    )?;
    ensure_table_column(conn, "chunks", "char_len", "INTEGER NOT NULL DEFAULT 0")?;
    Ok(())
}

fn ensure_file_index_state_schema(conn: &Connection) -> Result<(), StorageError> {
    ensure_table_column(
        conn,
        "file_index_state",
        "index_status",
        "TEXT NOT NULL DEFAULT 'ready'",
    )?;
    ensure_table_column(conn, "file_index_state", "last_error", "TEXT")?;
    ensure_table_column(
        conn,
        "file_index_state",
        "parser_format_version",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_table_column(
        conn,
        "file_index_state",
        "index_format_version",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    Ok(())
}

fn ensure_file_catalog_schema(conn: &Connection) -> Result<(), StorageError> {
    ensure_table_column(conn, "file_catalog", "relative_path", "TEXT NOT NULL DEFAULT ''")?;
    ensure_table_column(conn, "file_catalog", "parent_dir", "TEXT NOT NULL DEFAULT ''")?;
    ensure_table_column(conn, "file_catalog", "file_name", "TEXT NOT NULL DEFAULT ''")?;
    ensure_table_column(conn, "file_catalog", "file_ext", "TEXT NOT NULL DEFAULT ''")?;
    ensure_table_column(conn, "file_catalog", "file_size", "INTEGER NOT NULL DEFAULT 0")?;
    ensure_table_column(conn, "file_catalog", "mtime_secs", "INTEGER NOT NULL DEFAULT 0")?;
    ensure_table_column(
        conn,
        "file_catalog",
        "discovered_at",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_table_column(conn, "file_catalog", "removed_at", "INTEGER")?;
    Ok(())
}

fn ensure_table_column(
    conn: &Connection,
    table: &str,
    column: &str,
    column_def: &str,
) -> Result<(), StorageError> {
    let pragma = format!("PRAGMA table_info({table})");
    let mut stmt = conn.prepare(&pragma).map_err(map_sqlite_error)?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(map_sqlite_error)?;
    for row in rows {
        if row.map_err(map_sqlite_error)? == column {
            return Ok(());
        }
    }

    let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {column_def}");
    conn.execute(&sql, []).map_err(map_sqlite_error)?;
    Ok(())
}

fn ensure_system_metadata(conn: &Connection) -> Result<(), StorageError> {
    let has_existing_index_data = has_existing_index_data(conn)?;
    let stored_index_version = get_metadata_value(conn, METADATA_KEY_INDEX_FORMAT_VERSION)?
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0);
    let stored_parser_version = get_metadata_value(conn, METADATA_KEY_PARSER_FORMAT_VERSION)?
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0);
    let rebuild_state = get_metadata_value(conn, METADATA_KEY_REBUILD_STATE)?
        .map(|value| RebuildState::from_stored(&value))
        .unwrap_or(RebuildState::Ready);
    let rebuild_reason = get_metadata_value(conn, METADATA_KEY_REBUILD_REASON)?;
    let last_rebuild_at = get_metadata_value(conn, METADATA_KEY_LAST_REBUILD_AT)?;

    if stored_index_version == 0 && !has_existing_index_data {
        set_metadata_value(
            conn,
            METADATA_KEY_INDEX_FORMAT_VERSION,
            &INDEX_FORMAT_VERSION.to_string(),
        )?;
        set_metadata_value(
            conn,
            METADATA_KEY_PARSER_FORMAT_VERSION,
            &PARSER_FORMAT_VERSION.to_string(),
        )?;
        set_metadata_value(
            conn,
            METADATA_KEY_REBUILD_STATE,
            RebuildState::Ready.as_str(),
        )?;
        set_metadata_value(conn, METADATA_KEY_REBUILD_REASON, "")?;
        return Ok(());
    }

    if stored_index_version != INDEX_FORMAT_VERSION
        || stored_parser_version != PARSER_FORMAT_VERSION
    {
        if has_existing_index_data {
            set_metadata_value(
                conn,
                METADATA_KEY_REBUILD_STATE,
                RebuildState::Required.as_str(),
            )?;
            let reason = if stored_index_version == 0 || stored_parser_version == 0 {
                "index_metadata_missing"
            } else if stored_parser_version != PARSER_FORMAT_VERSION {
                "parser_format_changed"
            } else {
                "index_format_changed"
            };
            set_metadata_value(conn, METADATA_KEY_REBUILD_REASON, reason)?;
        } else {
            set_metadata_value(
                conn,
                METADATA_KEY_INDEX_FORMAT_VERSION,
                &INDEX_FORMAT_VERSION.to_string(),
            )?;
            set_metadata_value(
                conn,
                METADATA_KEY_PARSER_FORMAT_VERSION,
                &PARSER_FORMAT_VERSION.to_string(),
            )?;
            set_metadata_value(
                conn,
                METADATA_KEY_REBUILD_STATE,
                RebuildState::Ready.as_str(),
            )?;
            set_metadata_value(conn, METADATA_KEY_REBUILD_REASON, "")?;
        }
        return Ok(());
    }

    set_metadata_value(
        conn,
        METADATA_KEY_INDEX_FORMAT_VERSION,
        &stored_index_version.to_string(),
    )?;
    set_metadata_value(
        conn,
        METADATA_KEY_PARSER_FORMAT_VERSION,
        &stored_parser_version.to_string(),
    )?;
    set_metadata_value(conn, METADATA_KEY_REBUILD_STATE, rebuild_state.as_str())?;
    set_metadata_value(
        conn,
        METADATA_KEY_REBUILD_REASON,
        rebuild_reason.as_deref().unwrap_or(""),
    )?;
    if let Some(value) = last_rebuild_at {
        set_metadata_value(conn, METADATA_KEY_LAST_REBUILD_AT, &value)?;
    }
    Ok(())
}

fn has_existing_index_data(conn: &Connection) -> Result<bool, StorageError> {
    let documents: i64 = conn
        .query_row("SELECT COUNT(*) FROM documents", [], |row| row.get(0))
        .map_err(map_sqlite_error)?;
    let chunks: i64 = conn
        .query_row("SELECT COUNT(*) FROM chunks", [], |row| row.get(0))
        .map_err(map_sqlite_error)?;
    let file_index_state: i64 = conn
        .query_row("SELECT COUNT(*) FROM file_index_state", [], |row| {
            row.get(0)
        })
        .map_err(map_sqlite_error)?;
    Ok(documents > 0 || chunks > 0 || file_index_state > 0)
}

fn get_metadata_value(conn: &Connection, key: &str) -> Result<Option<String>, StorageError> {
    conn.query_row(
        "SELECT value FROM system_metadata WHERE key = ?1",
        params![key],
        |row| row.get(0),
    )
    .optional()
    .map_err(map_sqlite_error)
}

fn set_metadata_value(conn: &Connection, key: &str, value: &str) -> Result<(), StorageError> {
    conn.execute(
        "INSERT INTO system_metadata(key, value)
         VALUES(?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map_err(map_sqlite_error)?;
    Ok(())
}

fn read_index_metadata_from_conn(conn: &Connection) -> Result<IndexMetadata, StorageError> {
    let index_format_version = get_metadata_value(conn, METADATA_KEY_INDEX_FORMAT_VERSION)?
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0);
    let parser_format_version = get_metadata_value(conn, METADATA_KEY_PARSER_FORMAT_VERSION)?
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0);
    let rebuild_state = get_metadata_value(conn, METADATA_KEY_REBUILD_STATE)?
        .map(|value| RebuildState::from_stored(&value))
        .unwrap_or(RebuildState::Ready);
    let rebuild_reason = get_metadata_value(conn, METADATA_KEY_REBUILD_REASON)?.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });
    let last_rebuild_at = get_metadata_value(conn, METADATA_KEY_LAST_REBUILD_AT)?
        .and_then(|value| value.parse::<i64>().ok());

    Ok(IndexMetadata {
        index_format_version,
        parser_format_version,
        rebuild_state,
        rebuild_reason,
        last_rebuild_at,
    })
}

fn current_unix_timestamp_secs() -> Result<i64, StorageError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(StorageError::Clock)?;
    i64::try_from(duration.as_secs()).map_err(|_| StorageError::TimestampOverflow)
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }

    let len = a.len().min(b.len());
    if len == 0 {
        return 0.0;
    }

    let mut dot = 0.0_f32;
    let mut norm_a = 0.0_f32;
    let mut norm_b = 0.0_f32;

    for i in 0..len {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }

    let denominator = norm_a.sqrt() * norm_b.sqrt();
    if denominator == 0.0 {
        0.0
    } else {
        dot / denominator
    }
}

fn make_placeholders(count: usize) -> String {
    std::iter::repeat_n("?", count)
        .collect::<Vec<_>>()
        .join(",")
}

fn map_sqlite_error(err: rusqlite::Error) -> StorageError {
    match &err {
        rusqlite::Error::SqliteFailure(inner, _) => match inner.code {
            ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked => {
                StorageError::DatabaseLocked(err)
            }
            _ => StorageError::Sqlite(err),
        },
        _ => StorageError::Sqlite(err),
    }
}

fn directory_like_pattern(dir_path: &str) -> (String, String) {
    let trimmed = dir_path.trim_end_matches(['/', '\\']);
    let mut prefix = trimmed.to_string();

    let mut windows_prefix = prefix.clone();
    windows_prefix.push('\\');

    prefix.push('/');

    (
        format!("{}%", escape_like_pattern(&prefix)),
        format!("{}%", escape_like_pattern(&windows_prefix)),
    )
}

fn escape_like_pattern(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' | '%' | '_' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            _ => escaped.push(ch),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::{
        CatalogEntry, INDEX_FORMAT_VERSION, RebuildState, SqliteStore, build_document_search_text,
        extract_fts_terms, extract_phrase_signal_terms, extract_signal_terms,
    };
    use crate::VectorStore;
    use memori_parser::DocumentChunk;
    use memori_parser::PARSER_FORMAT_VERSION;
    use rusqlite::Connection;

    fn unique_db_path(prefix: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "{prefix}_{}.db",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("duration since epoch")
                .as_nanos()
        ))
    }

    #[tokio::test]
    async fn replace_document_index_round_trips_metadata_and_fts() {
        let db_path = unique_db_path("memori_vault_storage_roundtrip");
        if db_path.exists() {
            let _ = std::fs::remove_file(&db_path);
        }

        let store = SqliteStore::new(&db_path).expect("create sqlite store");
        let watch_root = std::path::PathBuf::from("notes");
        let file_path = watch_root.join("project").join("weekly.md");
        let chunks = vec![
            DocumentChunk {
                file_path: file_path.clone(),
                content: "Alpha rollout checklist".to_string(),
                chunk_index: 0,
                heading_path: vec!["Project".to_string(), "Weekly".to_string()],
                block_kind: memori_parser::ChunkBlockKind::Paragraph,
            },
            DocumentChunk {
                file_path: file_path.clone(),
                content: "```rust\nfn main() {}\n```".to_string(),
                chunk_index: 1,
                heading_path: vec!["Project".to_string(), "Implementation".to_string()],
                block_kind: memori_parser::ChunkBlockKind::CodeBlock,
            },
        ];

        store
            .replace_document_index(
                &file_path,
                Some(&watch_root),
                123,
                "content_hash_v1",
                chunks,
                vec![vec![1.0_f32, 0.0_f32], vec![0.0_f32, 1.0_f32]],
            )
            .await
            .expect("replace document index");

        let document = store
            .get_document_by_file_path(&file_path)
            .await
            .expect("get document")
            .expect("document exists");
        assert_eq!(document.relative_path, "project/weekly.md");
        assert_eq!(document.file_name, "weekly.md");
        assert_eq!(document.chunk_count, 2);
        assert!(
            document
                .document_search_text
                .contains("Alpha rollout checklist")
        );
        assert!(document.heading_catalog_text.contains("Project / Weekly"));

        let chunk_records = store
            .get_chunks_by_doc_id(document.id)
            .await
            .expect("get chunk records");
        assert_eq!(chunk_records.len(), 2);
        assert_eq!(
            chunk_records[0].heading_path,
            vec!["Project".to_string(), "Weekly".to_string()]
        );
        assert_eq!(chunk_records[1].block_kind, "code_block");

        let lexical_chunks = store
            .search_chunks_fts("Alpha weekly", 5, &[])
            .await
            .expect("search chunks fts");
        assert!(!lexical_chunks.is_empty());
        assert!(lexical_chunks.iter().any(|item| {
            item.file_name == "weekly.md"
                && item.heading_path == vec!["Project".to_string(), "Weekly".to_string()]
        }));

        let lexical_docs = store
            .search_documents_fts("project weekly", 5, &[])
            .await
            .expect("search documents fts");
        assert_eq!(lexical_docs.len(), 1);
        assert_eq!(lexical_docs[0].relative_path, "project/weekly.md");

        let scoped_docs = store
            .search_documents_fts("weekly", 5, &[watch_root.join("project")])
            .await
            .expect("search scoped docs");
        assert_eq!(scoped_docs.len(), 1);
        let blocked_docs = store
            .search_documents_fts("weekly", 5, &[watch_root.join("other")])
            .await
            .expect("search blocked docs");
        assert!(blocked_docs.is_empty());

        let signal_docs = store
            .search_documents_signal("weekly.md project", 5, &[])
            .await
            .expect("search deterministic docs");
        assert_eq!(signal_docs.len(), 1);
        assert!(
            signal_docs[0]
                .matched_fields
                .iter()
                .any(|field| field == "file_name")
        );
        assert!(
            signal_docs[0]
                .matched_fields
                .iter()
                .any(|field| field == "relative_path")
        );

        let semantic = store
            .search_similar_scoped(vec![1.0_f32, 0.0_f32], 1, &[])
            .await
            .expect("search similar scoped");
        assert_eq!(semantic.len(), 1);
        assert_eq!(
            semantic[0].0.heading_path,
            vec!["Project".to_string(), "Weekly".to_string()]
        );
        assert_eq!(
            semantic[0].0.block_kind,
            memori_parser::ChunkBlockKind::Paragraph
        );
        assert_eq!(
            store
                .embedding_dimension()
                .await
                .expect("embedding dimension"),
            Some(2)
        );

        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn document_search_text_samples_late_chunks_across_document() {
        let catalog_entry = CatalogEntry {
            file_path: "notes/project/weekly.md".to_string(),
            relative_path: "project/weekly.md".to_string(),
            parent_dir: "project".to_string(),
            file_name: "weekly.md".to_string(),
            file_ext: ".md".to_string(),
            file_size: 0,
            mtime_secs: 0,
            discovered_at: 0,
            removed_at: None,
        };
        let chunks = vec![
            DocumentChunk {
                file_path: std::path::PathBuf::from("notes/project/weekly.md"),
                content: "opening section ".repeat(40),
                chunk_index: 0,
                heading_path: vec!["Intro".to_string()],
                block_kind: memori_parser::ChunkBlockKind::Paragraph,
            },
            DocumentChunk {
                file_path: std::path::PathBuf::from("notes/project/weekly.md"),
                content: "late unique signal: settings Advanced tab".to_string(),
                chunk_index: 1,
                heading_path: vec!["Settings".to_string()],
                block_kind: memori_parser::ChunkBlockKind::Paragraph,
            },
        ];

        let document_search_text =
            build_document_search_text(&catalog_entry, "Intro\nSettings", &chunks);

        assert!(document_search_text.contains("weekly.md"));
        assert!(document_search_text.contains("late unique signal: settings Advanced tab"));
    }

    #[test]
    fn code_document_search_text_includes_symbols_and_literals() {
        let catalog_entry = CatalogEntry {
            file_path: "memori-core/src/lib.rs".to_string(),
            relative_path: "memori-core/src/lib.rs".to_string(),
            parent_dir: "memori-core/src".to_string(),
            file_name: "lib.rs".to_string(),
            file_ext: ".rs".to_string(),
            file_size: 0,
            mtime_secs: 0,
            discovered_at: 0,
            removed_at: None,
        };
        let chunks = vec![DocumentChunk {
            file_path: std::path::PathBuf::from("memori-core/src/lib.rs"),
            content: r#"
                pub struct RetrievalMetrics {
                    pub query_analysis_ms: u64,
                }

                pub async fn ask_vault_structured() {}
                const ASK_ROUTE: &str = "POST /api/ask";
            "#
            .to_string(),
            chunk_index: 0,
            heading_path: Vec::new(),
            block_kind: memori_parser::ChunkBlockKind::CodeBlock,
        }];

        let document_search_text = build_document_search_text(&catalog_entry, "", &chunks);

        assert!(document_search_text.contains("query_analysis_ms"));
        assert!(document_search_text.contains("ask_vault_structured"));
        assert!(document_search_text.contains("POST /api/ask"));
    }

    #[tokio::test]
    async fn search_documents_signal_matches_code_symbols_before_broad_text() {
        let db_path = unique_db_path("memori_vault_storage_code_signal");
        if db_path.exists() {
            let _ = std::fs::remove_file(&db_path);
        }

        let store = SqliteStore::new(&db_path).expect("create sqlite store");
        let watch_root = std::path::PathBuf::from(".");
        let code_file = std::path::PathBuf::from("memori-core/src/lib.rs");
        let readme_file = std::path::PathBuf::from("README.md");

        store
            .replace_document_index(
                &code_file,
                Some(&watch_root),
                123,
                "code_hash",
                vec![DocumentChunk {
                    file_path: code_file.clone(),
                    content: r#"
                        pub struct RetrievalMetrics {
                            pub query_analysis_ms: u64,
                        }

                        pub async fn ask_vault_structured() {}
                        const ASK_ROUTE: &str = "POST /api/ask";
                    "#
                    .to_string(),
                    chunk_index: 0,
                    heading_path: Vec::new(),
                    block_kind: memori_parser::ChunkBlockKind::CodeBlock,
                }],
                vec![vec![1.0_f32, 0.0_f32]],
            )
            .await
            .expect("index code file");

        store
            .replace_document_index(
                &readme_file,
                Some(&watch_root),
                123,
                "readme_hash",
                vec![DocumentChunk {
                    file_path: readme_file.clone(),
                    content: "This README mentions ask and api concepts in broad prose."
                        .to_string(),
                    chunk_index: 0,
                    heading_path: vec!["Overview".to_string()],
                    block_kind: memori_parser::ChunkBlockKind::Paragraph,
                }],
                vec![vec![0.0_f32, 1.0_f32]],
            )
            .await
            .expect("index readme file");

        let symbol_hits = store
            .search_documents_signal("ask_vault_structured POST /api/ask", 5, &[])
            .await
            .expect("search symbol docs");

        assert!(symbol_hits.first().is_some_and(|item| {
            item.file_path
                .replace('\\', "/")
                .ends_with("memori-core/src/lib.rs")
        }));
        assert!(
            symbol_hits[0]
                .matched_fields
                .iter()
                .any(|field| field == "exact_symbol")
        );

        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn search_documents_phrase_signal_prefers_docs_phrase_matches() {
        let db_path = unique_db_path("memori_vault_storage_phrase_signal");
        if db_path.exists() {
            let _ = std::fs::remove_file(&db_path);
        }

        let store = SqliteStore::new(&db_path).expect("create sqlite store");
        let watch_root = std::path::PathBuf::from(".");
        let tutorial_file = std::path::PathBuf::from("docs/TUTORIAL.md");
        let readme_file = std::path::PathBuf::from("README.md");

        store
            .replace_document_index(
                &tutorial_file,
                Some(&watch_root),
                123,
                "tutorial_hash",
                vec![DocumentChunk {
                    file_path: tutorial_file.clone(),
                    content: "How to start server mode:\n\ncargo run -p memori-server"
                        .to_string(),
                    chunk_index: 0,
                    heading_path: vec!["Server".to_string()],
                    block_kind: memori_parser::ChunkBlockKind::Paragraph,
                }],
                vec![vec![1.0_f32, 0.0_f32]],
            )
            .await
            .expect("index tutorial file");

        store
            .replace_document_index(
                &readme_file,
                Some(&watch_root),
                123,
                "readme_hash",
                vec![DocumentChunk {
                    file_path: readme_file.clone(),
                    content: "Server runtime overview and product summary.".to_string(),
                    chunk_index: 0,
                    heading_path: vec!["Overview".to_string()],
                    block_kind: memori_parser::ChunkBlockKind::Paragraph,
                }],
                vec![vec![0.0_f32, 1.0_f32]],
            )
            .await
            .expect("index readme file");

        let phrase_hits = store
            .search_documents_phrase_signal("How do you start server mode?", 5, &[])
            .await
            .expect("search docs phrase signal");

        assert!(
            phrase_hits.iter().any(|item| {
                item.file_path
                    .replace('\\', "/")
                    .to_ascii_lowercase()
                    .ends_with("docs/tutorial.md")
                    && item
                        .matched_fields
                        .iter()
                        .any(|field| field == "docs_phrase")
            }),
            "phrase_hits={phrase_hits:?}"
        );

        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn fts_terms_keep_cjk_meaningful_phrase_and_identifiers() {
        let terms = extract_fts_terms("长跳转公式是什么 POST /api/ask week8_report.md");
        assert!(terms.iter().any(|term| term == "长跳转公式"));
        assert!(!terms.iter().any(|term| term == "是什么"));
        assert!(terms.iter().any(|term| term == "post"));
        assert!(terms.iter().any(|term| term == "api"));
        assert!(terms.iter().any(|term| term == "ask"));
        assert!(terms.iter().any(|term| term == "week8_report.md"));
        assert!(terms.iter().any(|term| term == "week8_report"));
    }

    #[test]
    fn signal_terms_keep_mixed_cjk_and_digits() {
        let terms = extract_signal_terms("周报8 week8_report.md");
        assert!(terms.iter().any(|term| term == "周报8"));
        assert!(terms.iter().any(|term| term == "周报"));
        assert!(terms.iter().any(|term| term == "week8_report.md"));
        assert!(terms.iter().any(|term| term == "week8_report"));
    }

    #[test]
    fn phrase_signal_terms_keep_docs_and_api_phrases() {
        let terms = extract_phrase_signal_terms("What does POST /api/auth/oidc/login return?");
        assert!(terms.iter().any(|term| term == "post /api/auth/oidc/login"));

        let docs_terms = extract_phrase_signal_terms("How do you start server mode?");
        assert!(docs_terms.iter().any(|term| term == "start server mode"));
    }

    #[tokio::test]
    async fn purge_file_path_removes_document_chunks_and_index_state() {
        let db_path = std::env::temp_dir().join(format!(
            "memori_vault_storage_purge_{}.db",
            std::process::id()
        ));
        if db_path.exists() {
            let _ = std::fs::remove_file(&db_path);
        }

        let store = SqliteStore::new(&db_path).expect("create sqlite store");
        let file_path = std::path::PathBuf::from("notes/test.md");
        let chunks = vec![
            DocumentChunk {
                file_path: file_path.clone(),
                content: "hello".to_string(),
                chunk_index: 0,
                heading_path: Vec::new(),
                block_kind: memori_parser::ChunkBlockKind::Paragraph,
            },
            DocumentChunk {
                file_path: file_path.clone(),
                content: "world".to_string(),
                chunk_index: 1,
                heading_path: Vec::new(),
                block_kind: memori_parser::ChunkBlockKind::Paragraph,
            },
        ];
        let embeddings = vec![vec![0.1_f32, 0.2_f32], vec![0.3_f32, 0.4_f32]];

        store
            .insert_chunks(chunks, embeddings)
            .await
            .expect("insert chunks");
        store
            .upsert_file_index_state(&file_path, 10, 20, "hash")
            .await
            .expect("upsert file index state");

        let purged = store
            .purge_file_path(&file_path)
            .await
            .expect("purge file path");
        assert!(purged);

        assert!(
            store
                .resolve_chunk_id(&file_path, 0)
                .await
                .expect("resolve chunk id")
                .is_none()
        );
        assert!(
            store
                .get_file_index_state(&file_path)
                .await
                .expect("get file index state")
                .is_none()
        );

        let purged_again = store
            .purge_file_path(&file_path)
            .await
            .expect("purge missing file path");
        assert!(!purged_again);

        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn purge_directory_path_removes_nested_documents_and_index_state() {
        let db_path = std::env::temp_dir().join(format!(
            "memori_vault_storage_purge_dir_{}.db",
            std::process::id()
        ));
        if db_path.exists() {
            let _ = std::fs::remove_file(&db_path);
        }

        let store = SqliteStore::new(&db_path).expect("create sqlite store");
        let nested_a = std::path::PathBuf::from("notes/project/a.md");
        let nested_b = std::path::PathBuf::from("notes/project/sub/b.txt");
        let outside = std::path::PathBuf::from("notes/other/c.md");

        for file_path in [&nested_a, &nested_b, &outside] {
            store
                .insert_chunks(
                    vec![DocumentChunk {
                        file_path: file_path.clone(),
                        content: format!("content for {}", file_path.display()),
                        chunk_index: 0,
                        heading_path: Vec::new(),
                        block_kind: memori_parser::ChunkBlockKind::Paragraph,
                    }],
                    vec![vec![0.1_f32, 0.2_f32]],
                )
                .await
                .expect("insert chunks");
            store
                .upsert_file_index_state(file_path, 10, 20, "hash")
                .await
                .expect("upsert file index state");
        }

        let purged = store
            .purge_directory_path(&std::path::PathBuf::from("notes/project"))
            .await
            .expect("purge directory path");
        assert!(purged);

        assert!(
            store
                .resolve_chunk_id(&nested_a, 0)
                .await
                .expect("resolve nested a")
                .is_none()
        );
        assert!(
            store
                .resolve_chunk_id(&nested_b, 0)
                .await
                .expect("resolve nested b")
                .is_none()
        );
        assert!(
            store
                .get_file_index_state(&nested_a)
                .await
                .expect("state nested a")
                .is_none()
        );
        assert!(
            store
                .get_file_index_state(&nested_b)
                .await
                .expect("state nested b")
                .is_none()
        );
        assert!(
            store
                .resolve_chunk_id(&outside, 0)
                .await
                .expect("resolve outside")
                .is_some()
        );

        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn new_store_initializes_ready_index_metadata() {
        let db_path = std::env::temp_dir().join(format!(
            "memori_vault_store_meta_ready_{}.db",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("duration since epoch")
                .as_nanos()
        ));
        if db_path.exists() {
            let _ = std::fs::remove_file(&db_path);
        }

        let store = SqliteStore::new(&db_path).expect("create sqlite store");
        let metadata = store
            .read_index_metadata()
            .await
            .expect("read index metadata");

        assert_eq!(metadata.rebuild_state, RebuildState::Ready);
        assert_eq!(metadata.index_format_version, INDEX_FORMAT_VERSION);
        assert_eq!(metadata.parser_format_version, PARSER_FORMAT_VERSION);
        assert!(metadata.rebuild_reason.is_none());

        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn existing_index_without_metadata_is_marked_for_rebuild() {
        let db_path = std::env::temp_dir().join(format!(
            "memori_vault_store_meta_required_{}.db",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("duration since epoch")
                .as_nanos()
        ));
        if db_path.exists() {
            let _ = std::fs::remove_file(&db_path);
        }

        let conn = Connection::open(&db_path).expect("open raw sqlite db");
        conn.execute_batch(
            "CREATE TABLE documents (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 file_path TEXT NOT NULL UNIQUE,
                 last_modified INTEGER NOT NULL
             );
             CREATE TABLE chunks (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 doc_id INTEGER NOT NULL,
                 chunk_index INTEGER NOT NULL,
                 content TEXT NOT NULL,
                 embedding_blob BLOB NOT NULL
             );
             CREATE TABLE file_index_state (
                 file_path TEXT PRIMARY KEY,
                 file_size INTEGER NOT NULL,
                 mtime_secs INTEGER NOT NULL,
                 content_hash TEXT NOT NULL,
                 indexed_at INTEGER NOT NULL
             );
             CREATE TABLE graph_task_queue (
                 task_id INTEGER PRIMARY KEY AUTOINCREMENT,
                 chunk_id INTEGER NOT NULL,
                 content TEXT NOT NULL,
                 content_hash TEXT NOT NULL,
                 status TEXT NOT NULL,
                 retry_count INTEGER NOT NULL DEFAULT 0,
                 updated_at INTEGER NOT NULL
             );",
        )
        .expect("seed legacy schema");
        conn.execute(
            "INSERT INTO documents(file_path, last_modified) VALUES(?1, ?2)",
            rusqlite::params!["notes/legacy.md", 1_i64],
        )
        .expect("insert legacy doc");
        drop(conn);

        let store = SqliteStore::new(&db_path).expect("create sqlite store");
        let metadata = store
            .read_index_metadata()
            .await
            .expect("read index metadata");

        assert_eq!(metadata.rebuild_state, RebuildState::Required);
        assert_eq!(
            metadata.rebuild_reason.as_deref(),
            Some("index_metadata_missing")
        );
        assert_eq!(metadata.index_format_version, 0);
        assert_eq!(metadata.parser_format_version, 0);

        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn existing_legacy_file_catalog_is_upgraded_with_parent_dir_and_removed_at() {
        let db_path = unique_db_path("memori_vault_legacy_file_catalog_columns");
        if db_path.exists() {
            let _ = std::fs::remove_file(&db_path);
        }

        let conn = Connection::open(&db_path).expect("open raw sqlite db");
        conn.execute_batch(
            "CREATE TABLE file_catalog (
                 file_path TEXT PRIMARY KEY,
                 relative_path TEXT NOT NULL,
                 file_name TEXT NOT NULL,
                 file_ext TEXT NOT NULL,
                 file_size INTEGER NOT NULL,
                 mtime_secs INTEGER NOT NULL,
                 discovered_at INTEGER NOT NULL
             );",
        )
        .expect("seed legacy file_catalog schema");
        drop(conn);

        let store = SqliteStore::new(&db_path).expect("create sqlite store");
        let conn = store.lock_conn().expect("lock sqlite conn");
        let mut stmt = conn
            .prepare("PRAGMA table_info(file_catalog)")
            .expect("prepare pragma");
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .expect("query pragma")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect columns");

        assert!(columns.iter().any(|column| column == "parent_dir"));
        assert!(columns.iter().any(|column| column == "removed_at"));

        drop(stmt);
        drop(conn);
        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn purge_all_index_data_clears_relational_rows_and_cache() {
        let db_path = std::env::temp_dir().join(format!(
            "memori_vault_store_purge_all_{}.db",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("duration since epoch")
                .as_nanos()
        ));
        if db_path.exists() {
            let _ = std::fs::remove_file(&db_path);
        }

        let store = SqliteStore::new(&db_path).expect("create sqlite store");
        let file_path = std::path::PathBuf::from("notes/purge-all.md");
        store
            .insert_chunks(
                vec![DocumentChunk {
                    file_path: file_path.clone(),
                    content: "hello".to_string(),
                    chunk_index: 0,
                    heading_path: vec!["H1".to_string()],
                    block_kind: memori_parser::ChunkBlockKind::Paragraph,
                }],
                vec![vec![0.1_f32, 0.2_f32]],
            )
            .await
            .expect("insert chunks");
        store
            .upsert_file_index_state(&file_path, 11, 22, "hash")
            .await
            .expect("upsert file index");
        let chunk_id = store
            .resolve_chunk_id(&file_path, 0)
            .await
            .expect("resolve chunk id")
            .expect("chunk id exists");
        store
            .enqueue_graph_task(chunk_id, "hash", "hello")
            .await
            .expect("enqueue graph task");

        store
            .purge_all_index_data()
            .await
            .expect("purge all index data");

        assert_eq!(store.count_documents().await.expect("count documents"), 0);
        assert_eq!(store.count_chunks().await.expect("count chunks"), 0);
        assert_eq!(
            store
                .count_graph_backlog()
                .await
                .expect("count graph backlog"),
            0
        );
        assert!(
            store
                .get_file_index_state(&file_path)
                .await
                .expect("get file index state")
                .is_none()
        );
        assert_eq!(store.load_from_db().await.expect("load from db"), 0);

        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }
}

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
