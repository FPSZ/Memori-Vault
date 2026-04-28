use super::*;

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

    pub(crate) fn lock_conn(&self) -> Result<std::sync::MutexGuard<'_, Connection>, StorageError> {
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
