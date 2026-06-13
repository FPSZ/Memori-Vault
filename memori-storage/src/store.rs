use super::*;
use rusqlite::OpenFlags;

/// 统一连接 PRAGMA：WAL（读写不互斥）+ busy_timeout（避免瞬时锁冲突直接报错）+
/// synchronous=NORMAL（WAL 下安全且更快）。写连接额外开 foreign_keys。
fn configure_connection(conn: &Connection, read_only: bool) -> Result<(), StorageError> {
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(map_sqlite_error)?;
    // 只读连接对 WAL 库设置 journal_mode 是无害 no-op；写连接真正切到 WAL。
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(map_sqlite_error)?;
    conn.pragma_update(None, "synchronous", "NORMAL")
        .map_err(map_sqlite_error)?;
    if !read_only {
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(map_sqlite_error)?;
    }
    Ok(())
}

/// 解析只读连接池大小：env `MEMORI_DB_READ_POOL_SIZE` 优先，缺省 4，上限 32。
fn resolve_read_pool_size() -> usize {
    std::env::var("MEMORI_DB_READ_POOL_SIZE")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(SqliteStore::DEFAULT_READ_POOL_SIZE)
        .min(32)
}

impl SqliteStore {
    /// 默认只读连接池大小（审计 P1：缓解单连接串行化）。可经
    /// `MEMORI_DB_READ_POOL_SIZE` 覆盖；设 0 关闭池、检索回退写连接。
    const DEFAULT_READ_POOL_SIZE: usize = 4;

    /// 打开数据库并初始化表结构。
    pub fn new(db_path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let db_path = db_path.as_ref();
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(StorageError::Io)?;
        }

        // 写连接：建库 + WAL，使读写不互斥（WAL 下读者不阻塞单写者，反之亦然）。
        let write_conn = Connection::open(db_path).map_err(map_sqlite_error)?;
        configure_connection(&write_conn, false)?;
        initialize_schema(&write_conn)?;

        // 只读连接池：schema 建好后再开，WAL 下并发读检索热路径。
        let pool_size = resolve_read_pool_size();
        let mut read_pool = Vec::with_capacity(pool_size);
        for _ in 0..pool_size {
            let read_conn = Connection::open_with_flags(
                db_path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(map_sqlite_error)?;
            configure_connection(&read_conn, true)?;
            read_pool.push(Mutex::new(read_conn));
        }

        Ok(Self {
            write_conn: Mutex::new(write_conn),
            read_pool,
            read_cursor: std::sync::atomic::AtomicUsize::new(0),
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
            tx.execute("DELETE FROM chunks", [])
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
        let can_load_preserved_index = metadata
            .rebuild_reason
            .as_deref()
            .map(|reason| {
                reason.contains("retryable_files_remaining")
                    || reason.starts_with("rebuild_failed:Index unavailable")
                    || reason.starts_with("rebuild_failed:index is not ready")
                    || reason.starts_with("rebuild_failed:索引不可用")
            })
            .unwrap_or(false);
        if metadata.rebuild_state != RebuildState::Ready && !can_load_preserved_index {
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

    /// 写连接锁：所有写、以及非检索读走这里（串行）。
    pub(crate) fn lock_conn(&self) -> Result<std::sync::MutexGuard<'_, Connection>, StorageError> {
        self.write_conn
            .lock()
            .map_err(|_| StorageError::LockPoisoned("sqlite connection"))
    }

    /// 只读连接锁：检索热路径专用。轮询池内连接，try_lock 命中空闲即返回；
    /// 全忙则阻塞等待游标指向的那个。池为空时回退写连接（保持改造前行为）。
    /// WAL 下读者读到的是最后一次已提交事务——索引提交后检索可见，满足一致性。
    pub(crate) fn lock_read_conn(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, Connection>, StorageError> {
        if self.read_pool.is_empty() {
            return self.lock_conn();
        }
        let len = self.read_pool.len();
        let start = self
            .read_cursor
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            % len;
        for offset in 0..len {
            let idx = (start + offset) % len;
            if let Ok(guard) = self.read_pool[idx].try_lock() {
                return Ok(guard);
            }
        }
        // 全忙：阻塞在轮询起点那个，避免忙等。
        self.read_pool[start]
            .lock()
            .map_err(|_| StorageError::LockPoisoned("sqlite read connection"))
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
            .query_row(
                "SELECT COUNT(DISTINCT n.id)
                 FROM nodes n
                 INNER JOIN chunk_nodes cn ON cn.node_id = n.id
                 INNER JOIN chunks c ON c.id = cn.chunk_id
                 INNER JOIN documents d ON d.id = c.doc_id
                 INNER JOIN file_catalog fc ON fc.file_path = d.file_path
                 WHERE fc.removed_at IS NULL",
                [],
                |row| row.get(0),
            )
            .map_err(map_sqlite_error)?;
        u64::try_from(count).map_err(|_| StorageError::NegativeCount {
            table: "nodes",
            count,
        })
    }

    /// 统计 edges 表总行数。
    pub async fn count_edges(&self) -> Result<u64, StorageError> {
        let conn_guard = self.lock_conn()?;
        let count: i64 = conn_guard
            .query_row("SELECT COUNT(*) FROM edges", [], |row| row.get(0))
            .map_err(map_sqlite_error)?;
        u64::try_from(count).map_err(|_| StorageError::NegativeCount {
            table: "edges",
            count,
        })
    }

    /// 统计 file_catalog 表总行数（不含已删除的）。
    pub async fn count_catalog_entries(&self) -> Result<u64, StorageError> {
        let conn_guard = self.lock_conn()?;
        let count: i64 = conn_guard
            .query_row(
                "SELECT COUNT(*) FROM file_catalog WHERE removed_at IS NULL",
                [],
                |row| row.get(0),
            )
            .map_err(map_sqlite_error)?;
        u64::try_from(count).map_err(|_| StorageError::NegativeCount {
            table: "file_catalog",
            count,
        })
    }
}
