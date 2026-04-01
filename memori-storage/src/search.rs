use super::*;
use crate::document::{chunk_block_kind_from_storage, parse_heading_path_json};

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

impl SqliteStore {
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
                phrase_specific: false,
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

            let Some((score, matched_fields, phrase_specific)) = score_document_phrase_signal_match(
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
                phrase_specific,
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
