use super::*;
use crate::document::chunk_block_kind_to_storage;

impl SqliteStore {
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
            for (chunk, embedding) in chunks.into_iter().zip(embeddings) {
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

    pub async fn list_retryable_file_index_paths(&self) -> Result<Vec<PathBuf>, StorageError> {
        let conn_guard = self.lock_conn()?;
        let mut stmt = conn_guard
            .prepare(
                "SELECT fis.file_path
                 FROM file_index_state fis
                 INNER JOIN file_catalog fc ON fc.file_path = fis.file_path
                 WHERE fis.index_status IN ('pending', 'failed')
                   AND fc.removed_at IS NULL
                 ORDER BY fis.indexed_at ASC, fis.file_path ASC",
            )
            .map_err(map_sqlite_error)?;

        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(map_sqlite_error)?;

        let mut paths = Vec::new();
        for row in rows {
            paths.push(PathBuf::from(row.map_err(map_sqlite_error)?));
        }
        Ok(paths)
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

    pub(crate) async fn upsert_file_index_state_internal(
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

}
