use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use memori_parser::DocumentChunk;
use rusqlite::{Connection, ErrorCode, OptionalExtension, params, params_from_iter};
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileIndexState {
    pub file_path: String,
    pub file_size: i64,
    pub mtime_secs: i64,
    pub content_hash: String,
    pub indexed_at: i64,
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

    /// 冷启动时从 DB 读取全部向量到内存缓存。
    /// 返回值为加载到缓存中的向量条数。
    pub async fn load_from_db(&self) -> Result<usize, StorageError> {
        let loaded = {
            let conn_guard = self.lock_conn()?;
            let mut stmt = conn_guard
                .prepare(
                    "SELECT c.id, c.doc_id, c.embedding_blob, d.file_path
                     FROM chunks c
                     INNER JOIN documents d ON d.id = c.doc_id",
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
        let file_path_text = file_path.to_string_lossy().to_string();
        let conn_guard = self.lock_conn()?;
        let row = conn_guard
            .query_row(
                "SELECT file_path, file_size, mtime_secs, content_hash, indexed_at
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
        let file_path_text = file_path.to_string_lossy().to_string();
        let now = current_unix_timestamp_secs()?;
        let conn_guard = self.lock_conn()?;
        conn_guard
            .execute(
                "INSERT INTO file_index_state(file_path, file_size, mtime_secs, content_hash, indexed_at)
                 VALUES(?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(file_path) DO UPDATE SET
                   file_size = excluded.file_size,
                   mtime_secs = excluded.mtime_secs,
                   content_hash = excluded.content_hash,
                   indexed_at = excluded.indexed_at",
                params![file_path_text, file_size, mtime_secs, content_hash, now],
            )
            .map_err(map_sqlite_error)?;
        Ok(())
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
                    file_path: file_path.clone(),
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
         CREATE TABLE IF NOT EXISTS file_index_state (
             file_path TEXT PRIMARY KEY,
             file_size INTEGER NOT NULL,
             mtime_secs INTEGER NOT NULL,
             content_hash TEXT NOT NULL,
             indexed_at INTEGER NOT NULL
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
         CREATE INDEX IF NOT EXISTS idx_chunks_doc_id ON chunks(doc_id);
         CREATE UNIQUE INDEX IF NOT EXISTS idx_chunks_doc_chunk_index ON chunks(doc_id, chunk_index);
         CREATE INDEX IF NOT EXISTS idx_edges_source ON edges(source_node);
         CREATE INDEX IF NOT EXISTS idx_edges_target ON edges(target_node);
         CREATE INDEX IF NOT EXISTS idx_chunk_nodes_chunk_id ON chunk_nodes(chunk_id);
         CREATE INDEX IF NOT EXISTS idx_chunk_nodes_node_id ON chunk_nodes(node_id);
         CREATE INDEX IF NOT EXISTS idx_graph_task_queue_status ON graph_task_queue(status, updated_at);
         CREATE INDEX IF NOT EXISTS idx_file_index_state_indexed_at ON file_index_state(indexed_at);",
    )
    .map_err(map_sqlite_error)?;
    ensure_graph_task_queue_schema(conn)?;
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

    #[error("统计结果异常，表 {table} 出现负数计数: {count}")]
    NegativeCount { table: &'static str, count: i64 },
}
