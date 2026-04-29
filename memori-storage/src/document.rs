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

    pub async fn search_graph_nodes(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<GraphNode>, StorageError> {
        let trimmed = query.trim();
        if trimmed.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let limit = i64::try_from(limit.min(50)).unwrap_or(20);
        let pattern = format!("%{}%", trimmed.replace('%', "\\%").replace('_', "\\_"));
        let conn_guard = self.lock_conn()?;
        let mut stmt = conn_guard
            .prepare(
                "SELECT id, label, name, description
                 FROM nodes
                 WHERE id LIKE ?1 ESCAPE '\\'
                    OR label LIKE ?1 ESCAPE '\\'
                    OR name LIKE ?1 ESCAPE '\\'
                    OR COALESCE(description, '') LIKE ?1 ESCAPE '\\'
                 ORDER BY name ASC, id ASC
                 LIMIT ?2",
            )
            .map_err(map_sqlite_error)?;
        let rows = stmt
            .query_map(params![pattern, limit], |row| {
                Ok(GraphNode {
                    id: row.get(0)?,
                    label: row.get(1)?,
                    name: row.get(2)?,
                    description: row.get(3)?,
                })
            })
            .map_err(map_sqlite_error)?;
        let mut nodes = Vec::new();
        for row in rows {
            nodes.push(row.map_err(map_sqlite_error)?);
        }
        Ok(nodes)
    }

    pub async fn get_graph_neighbors(
        &self,
        node_id: &str,
        limit: usize,
    ) -> Result<GraphNeighbors, StorageError> {
        let node_id = node_id.trim();
        if node_id.is_empty() {
            return Ok(GraphNeighbors::default());
        }
        let limit = i64::try_from(limit.min(100)).unwrap_or(30);
        let conn_guard = self.lock_conn()?;
        let center = conn_guard
            .query_row(
                "SELECT id, label, name, description FROM nodes WHERE id = ?1",
                params![node_id],
                |row| {
                    Ok(GraphNode {
                        id: row.get(0)?,
                        label: row.get(1)?,
                        name: row.get(2)?,
                        description: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(map_sqlite_error)?;

        let mut edge_stmt = conn_guard
            .prepare(
                "SELECT id, source_node, target_node, relation
                 FROM edges
                 WHERE source_node = ?1 OR target_node = ?1
                 ORDER BY relation ASC, id ASC
                 LIMIT ?2",
            )
            .map_err(map_sqlite_error)?;
        let edge_rows = edge_stmt
            .query_map(params![node_id, limit], |row| {
                Ok(GraphEdge {
                    id: row.get(0)?,
                    source_node: row.get(1)?,
                    target_node: row.get(2)?,
                    relation: row.get(3)?,
                })
            })
            .map_err(map_sqlite_error)?;
        let mut edges = Vec::new();
        let mut neighbor_ids = HashSet::new();
        for row in edge_rows {
            let edge = row.map_err(map_sqlite_error)?;
            if edge.source_node == node_id {
                neighbor_ids.insert(edge.target_node.clone());
            } else {
                neighbor_ids.insert(edge.source_node.clone());
            }
            edges.push(edge);
        }

        let mut nodes = Vec::new();
        if !neighbor_ids.is_empty() {
            let ids = neighbor_ids.into_iter().collect::<Vec<_>>();
            let placeholders = make_placeholders(ids.len());
            let query = format!(
                "SELECT id, label, name, description FROM nodes WHERE id IN ({})",
                placeholders
            );
            let mut node_stmt = conn_guard.prepare(&query).map_err(map_sqlite_error)?;
            let node_rows = node_stmt
                .query_map(params_from_iter(ids.iter()), |row| {
                    Ok(GraphNode {
                        id: row.get(0)?,
                        label: row.get(1)?,
                        name: row.get(2)?,
                        description: row.get(3)?,
                    })
                })
                .map_err(map_sqlite_error)?;
            for row in node_rows {
                nodes.push(row.map_err(map_sqlite_error)?);
            }
        }

        let mut source_chunks = Vec::new();
        let mut chunk_stmt = conn_guard
            .prepare(
                "SELECT c.id, c.doc_id, c.chunk_index, c.content, c.heading_path_json, c.block_kind, c.char_len
                 FROM chunk_nodes cn
                 INNER JOIN chunks c ON c.id = cn.chunk_id
                 WHERE cn.node_id = ?1
                 ORDER BY c.id ASC
                 LIMIT 20",
            )
            .map_err(map_sqlite_error)?;
        let chunk_rows = chunk_stmt
            .query_map(params![node_id], map_chunk_record)
            .map_err(map_sqlite_error)?;
        for row in chunk_rows {
            source_chunks.push(row.map_err(map_sqlite_error)?);
        }

        Ok(GraphNeighbors {
            center,
            nodes,
            edges,
            source_chunks,
        })
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

    pub async fn reset_running_graph_tasks(&self) -> Result<u64, StorageError> {
        let now = current_unix_timestamp_secs()?;
        let conn_guard = self.lock_conn()?;
        let changed = conn_guard
            .execute(
                "UPDATE graph_task_queue
                 SET status = 'pending', updated_at = ?1
                 WHERE status = 'running'",
                params![now],
            )
            .map_err(map_sqlite_error)?;
        u64::try_from(changed).map_err(|_| StorageError::NegativeCount {
            table: "graph_task_queue",
            count: changed as i64,
        })
    }

    pub async fn mark_orphan_graph_tasks_done(&self) -> Result<u64, StorageError> {
        let now = current_unix_timestamp_secs()?;
        let conn_guard = self.lock_conn()?;
        let changed = conn_guard
            .execute(
                "UPDATE graph_task_queue
                 SET status = 'done', updated_at = ?1
                 WHERE status IN ('pending', 'running')
                   AND NOT EXISTS (
                     SELECT 1 FROM chunks c WHERE c.id = graph_task_queue.chunk_id
                   )",
                params![now],
            )
            .map_err(map_sqlite_error)?;
        u64::try_from(changed).map_err(|_| StorageError::NegativeCount {
            table: "graph_task_queue",
            count: changed as i64,
        })
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
