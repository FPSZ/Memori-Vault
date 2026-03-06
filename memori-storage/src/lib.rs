use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use memori_parser::DocumentChunk;
use rusqlite::{Connection, ErrorCode, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::info;

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

#[derive(Debug, Clone)]
struct CachedVector {
    chunk_id: i64,
    doc_id: i64,
    embedding: Vec<f32>,
}

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

    /// 冷启动时从 DB 读取全部向量到内存缓存。
    /// 返回值为加载到缓存中的向量条数。
    pub async fn load_from_db(&self) -> Result<usize, StorageError> {
        let loaded = {
            let conn_guard = self.lock_conn()?;
            let mut stmt = conn_guard
                .prepare("SELECT id, doc_id, embedding_blob FROM chunks")
                .map_err(map_sqlite_error)?;

            let mut rows = stmt.query([]).map_err(map_sqlite_error)?;
            let mut loaded = Vec::new();

            while let Some(row) = rows.next().map_err(map_sqlite_error)? {
                let chunk_id: i64 = row.get(0).map_err(map_sqlite_error)?;
                let doc_id: i64 = row.get(1).map_err(map_sqlite_error)?;
                let blob: Vec<u8> = row.get(2).map_err(map_sqlite_error)?;
                let embedding: Vec<f32> =
                    bincode::deserialize(&blob).map_err(StorageError::DeserializeEmbedding)?;

                loaded.push(CachedVector {
                    chunk_id,
                    doc_id,
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

        let file_path_text = file_path.to_string_lossy().to_string();
        let conn_guard = self.lock_conn()?;

        let chunk_id = conn_guard
            .query_row(
                "SELECT c.id
                 FROM chunks c
                 INNER JOIN documents d ON d.id = c.doc_id
                 WHERE d.file_path = ?1 AND c.chunk_index = ?2",
                params![file_path_text, chunk_index],
                |row| row.get(0),
            )
            .optional()
            .map_err(map_sqlite_error)?;

        Ok(chunk_id)
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

            valid_nodes += 1;
        }

        let mut valid_edges = 0usize;
        for edge in &edges {
            if edge.id.trim().is_empty()
                || edge.source_node.trim().is_empty()
                || edge.target_node.trim().is_empty()
                || edge.relation.trim().is_empty()
            {
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
            "图谱数据写入完成"
        );

        Ok(())
    }

    fn lock_conn(&self) -> Result<std::sync::MutexGuard<'_, Connection>, StorageError> {
        self.conn
            .lock()
            .map_err(|_| StorageError::LockPoisoned("sqlite connection"))
    }
}

impl VectorStore for SqliteStore {
    async fn insert_chunks(
        &self,
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

        let file_path = chunks[0].file_path.clone();
        if chunks.iter().any(|chunk| chunk.file_path != file_path) {
            return Err(StorageError::MixedFilePathInBatch);
        }

        let last_modified = current_unix_timestamp_secs()?;
        let file_path_text = file_path.to_string_lossy().to_string();

        let (doc_id, inserted_cache_rows) = {
            let mut conn_guard = self.lock_conn()?;
            let tx = conn_guard.transaction().map_err(map_sqlite_error)?;

            tx.execute(
                "INSERT INTO documents(file_path, last_modified) VALUES(?1, ?2)
                 ON CONFLICT(file_path) DO UPDATE SET last_modified = excluded.last_modified",
                params![file_path_text, last_modified],
            )
            .map_err(map_sqlite_error)?;

            let doc_id: i64 = tx
                .query_row(
                    "SELECT id FROM documents WHERE file_path = ?1",
                    params![file_path_text],
                    |row| row.get(0),
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
                let embedding_blob =
                    bincode::serialize(&embedding).map_err(StorageError::SerializeEmbedding)?;

                tx.execute(
                    "INSERT INTO chunks(doc_id, chunk_index, content, embedding_blob)
                     VALUES(?1, ?2, ?3, ?4)",
                    params![doc_id, chunk_index, chunk.content, embedding_blob],
                )
                .map_err(map_sqlite_error)?;

                let chunk_id = tx.last_insert_rowid();
                inserted.push(CachedVector {
                    chunk_id,
                    doc_id,
                    embedding,
                });
            }

            tx.commit().map_err(map_sqlite_error)?;
            (doc_id, inserted)
        };

        let mut cache_guard = self.cache.write().await;
        cache_guard.retain(|item| item.doc_id != doc_id);
        cache_guard.extend(inserted_cache_rows);

        info!(
            file_path = %file_path.display(),
            inserted = chunk_count,
            total_vectors = cache_guard.len(),
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

        let mut top = {
            let cache_guard = self.cache.read().await;
            let mut scored: Vec<(i64, f32)> = cache_guard
                .iter()
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
                "SELECT c.chunk_index, c.content, d.file_path
                 FROM chunks c
                 INNER JOIN documents d ON d.id = c.doc_id
                 WHERE c.id = ?1",
            )
            .map_err(map_sqlite_error)?;

        let mut results = Vec::with_capacity(top.len());
        for (chunk_id, score) in top.drain(..) {
            let row = stmt
                .query_row(params![chunk_id], |row| {
                    let chunk_index_raw: i64 = row.get(0)?;
                    let content: String = row.get(1)?;
                    let file_path: String = row.get(2)?;
                    Ok((chunk_index_raw, content, file_path))
                })
                .optional()
                .map_err(map_sqlite_error)?;

            if let Some((chunk_index_raw, content, file_path_text)) = row {
                let chunk_index = usize::try_from(chunk_index_raw).map_err(|_| {
                    StorageError::InvalidChunkIndex {
                        raw: chunk_index_raw,
                    }
                })?;

                results.push((
                    DocumentChunk {
                        file_path: PathBuf::from(file_path_text),
                        content,
                        chunk_index,
                    },
                    score,
                ));
            }
        }

        Ok(results)
    }
}

fn initialize_schema(conn: &Connection) -> Result<(), StorageError> {
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         CREATE TABLE IF NOT EXISTS documents (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             file_path TEXT NOT NULL UNIQUE,
             last_modified INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS chunks (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             doc_id INTEGER NOT NULL,
             chunk_index INTEGER NOT NULL,
             content TEXT NOT NULL,
             embedding_blob BLOB NOT NULL,
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
         CREATE INDEX IF NOT EXISTS idx_chunks_doc_id ON chunks(doc_id);
         CREATE UNIQUE INDEX IF NOT EXISTS idx_chunks_doc_chunk_index ON chunks(doc_id, chunk_index);
         CREATE INDEX IF NOT EXISTS idx_edges_source ON edges(source_node);
         CREATE INDEX IF NOT EXISTS idx_edges_target ON edges(target_node);
         CREATE INDEX IF NOT EXISTS idx_chunk_nodes_chunk_id ON chunk_nodes(chunk_id);
         CREATE INDEX IF NOT EXISTS idx_chunk_nodes_node_id ON chunk_nodes(node_id);",
    )
    .map_err(map_sqlite_error)
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

    #[error("Embedding 序列化失败: {0}")]
    SerializeEmbedding(#[source] bincode::Error),

    #[error("Embedding 反序列化失败: {0}")]
    DeserializeEmbedding(#[source] bincode::Error),

    #[error("系统时间异常: {0}")]
    Clock(#[source] std::time::SystemTimeError),

    #[error("时间戳溢出")]
    TimestampOverflow,

    #[error("IO 操作失败: {0}")]
    Io(#[source] std::io::Error),
}
