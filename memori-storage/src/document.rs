use super::*;

impl SqliteStore {
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

    pub async fn list_active_catalog_file_paths(&self) -> Result<Vec<PathBuf>, StorageError> {
        let conn_guard = self.lock_conn()?;
        let mut stmt = conn_guard
            .prepare(
                "SELECT file_path
                 FROM file_catalog
                 WHERE removed_at IS NULL
                 ORDER BY file_path ASC",
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

    pub async fn get_chunk_by_id(
        &self,
        chunk_id: i64,
    ) -> Result<Option<ChunkRecord>, StorageError> {
        let conn_guard = self.lock_conn()?;
        conn_guard
            .query_row(
                "SELECT id, doc_id, chunk_index, content, heading_path_json, block_kind, char_len
                 FROM chunks
                 WHERE id = ?1",
                params![chunk_id],
                map_chunk_record,
            )
            .optional()
            .map_err(map_sqlite_error)
    }

    pub async fn get_document_by_id(
        &self,
        doc_id: i64,
    ) -> Result<Option<DocumentRecord>, StorageError> {
        let conn_guard = self.lock_conn()?;
        conn_guard
            .query_row(
                "SELECT d.id, d.file_path, d.relative_path, d.file_name, d.file_ext, d.last_modified, d.indexed_at,
                        d.chunk_count, d.content_char_count, d.heading_catalog_text, d.document_search_text
                 FROM documents d
                 INNER JOIN file_catalog fc ON fc.file_path = d.file_path
                 WHERE d.id = ?1
                   AND fc.removed_at IS NULL",
                params![doc_id],
                map_document_record,
            )
            .optional()
            .map_err(map_sqlite_error)
    }

}

pub(crate) fn parse_heading_path_json(raw: &str) -> Result<Vec<String>, StorageError> {
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(raw).map_err(StorageError::SerdeJson)
}

pub(crate) fn chunk_block_kind_to_storage(kind: ChunkBlockKind) -> &'static str {
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

pub(crate) fn chunk_block_kind_from_storage(raw: &str) -> ChunkBlockKind {
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

pub(crate) fn map_document_record(
    row: &rusqlite::Row<'_>,
) -> Result<DocumentRecord, rusqlite::Error> {
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

pub(crate) fn map_chunk_record(row: &rusqlite::Row<'_>) -> Result<ChunkRecord, rusqlite::Error> {
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
