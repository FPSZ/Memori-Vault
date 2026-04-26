use super::*;
use std::hash::{Hash, Hasher};

impl SqliteStore {
    pub async fn insert_memory_event(
        &self,
        event: NewMemoryEvent,
    ) -> Result<MemoryEventRecord, StorageError> {
        let now = current_unix_timestamp_secs()?;
        let content_hash = stable_text_hash(&event.content);
        let conn_guard = self.lock_conn()?;
        conn_guard
            .execute(
                "INSERT INTO memory_events(scope, scope_id, event_type, content, source_ref, content_hash, created_at)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    event.scope.as_str(),
                    event.scope_id,
                    event.event_type,
                    event.content,
                    event.source_ref,
                    content_hash,
                    now,
                ],
            )
            .map_err(map_sqlite_error)?;
        let id = conn_guard.last_insert_rowid();
        self.get_memory_event_by_id_with_conn(&conn_guard, id)
    }

    pub async fn add_memory(&self, memory: NewMemoryRecord) -> Result<MemoryRecord, StorageError> {
        let now = current_unix_timestamp_secs()?;
        let mut conn_guard = self.lock_conn()?;
        let tx = conn_guard.transaction().map_err(map_sqlite_error)?;
        tx.execute(
            "INSERT INTO memories(
                layer, scope, scope_id, memory_type, title, content, source_type, source_ref,
                confidence, status, tags_json, links_json, supersedes, created_at, updated_at
             )
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                memory.layer.as_str(),
                memory.scope.as_str(),
                memory.scope_id,
                memory.memory_type,
                memory.title,
                memory.content,
                memory.source_type.as_str(),
                memory.source_ref,
                memory.confidence.clamp(0.0, 1.0),
                memory.status.as_str(),
                serde_json::to_string(&memory.tags).map_err(StorageError::SerdeJson)?,
                serde_json::to_string(&memory.links).map_err(StorageError::SerdeJson)?,
                memory.supersedes,
                now,
                now,
            ],
        )
        .map_err(map_sqlite_error)?;
        let id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO memory_lifecycle_log(action, target_type, target_id, reason, model, source_ref, created_at)
             VALUES(?1, 'memory', ?2, ?3, ?4, ?5, ?6)",
            params![
                LifecycleAction::Add.as_str(),
                id,
                memory.reason,
                memory.model,
                memory.source_ref,
                now,
            ],
        )
        .map_err(map_sqlite_error)?;
        tx.commit().map_err(map_sqlite_error)?;
        query_memory_by_id(&conn_guard, id)
    }

    pub async fn update_memory(
        &self,
        id: i64,
        update: UpdateMemoryRecord,
    ) -> Result<Option<MemoryRecord>, StorageError> {
        let now = current_unix_timestamp_secs()?;
        let mut conn_guard = self.lock_conn()?;
        let tx = conn_guard.transaction().map_err(map_sqlite_error)?;
        let existing = query_memory_by_id(&tx, id).optional_like_not_found()?;
        let Some(existing) = existing else {
            return Ok(None);
        };
        let next_title = update.title.unwrap_or(existing.title);
        let next_content = update.content.unwrap_or(existing.content);
        let next_status = update.status.unwrap_or(existing.status);
        let next_supersedes = update.supersedes.or(existing.supersedes);
        tx.execute(
            "UPDATE memories
             SET title = ?1, content = ?2, status = ?3, supersedes = ?4, updated_at = ?5
             WHERE id = ?6",
            params![
                next_title,
                next_content,
                next_status.as_str(),
                next_supersedes,
                now,
                id,
            ],
        )
        .map_err(map_sqlite_error)?;
        let action = if next_status == MemoryStatus::Superseded {
            LifecycleAction::Supersede
        } else if next_status == MemoryStatus::Deleted {
            LifecycleAction::Delete
        } else {
            LifecycleAction::Update
        };
        tx.execute(
            "INSERT INTO memory_lifecycle_log(action, target_type, target_id, reason, model, source_ref, created_at)
             VALUES(?1, 'memory', ?2, ?3, ?4, ?5, ?6)",
            params![
                action.as_str(),
                id,
                update.reason,
                update.model,
                existing.source_ref,
                now,
            ],
        )
        .map_err(map_sqlite_error)?;
        tx.commit().map_err(map_sqlite_error)?;
        query_memory_by_id(&conn_guard, id).map(Some)
    }

    pub async fn get_memory_by_id(&self, id: i64) -> Result<MemoryRecord, StorageError> {
        let conn_guard = self.lock_conn()?;
        query_memory_by_id(&conn_guard, id)
    }

    pub async fn list_recent_memories(
        &self,
        scope: Option<MemoryScope>,
        limit: usize,
    ) -> Result<Vec<MemoryRecord>, StorageError> {
        let limit = limit.clamp(1, 100);
        let conn_guard = self.lock_conn()?;
        let mut records = Vec::new();
        if let Some(scope) = scope {
            let mut stmt = conn_guard
                .prepare(
                    "SELECT id, layer, scope, scope_id, memory_type, title, content, source_type, source_ref,
                            confidence, status, tags_json, links_json, supersedes, created_at, updated_at
                     FROM memories
                     WHERE scope = ?1 AND status != 'deleted'
                     ORDER BY updated_at DESC, id DESC
                     LIMIT ?2",
                )
                .map_err(map_sqlite_error)?;
            let rows = stmt
                .query_map(params![scope.as_str(), limit as i64], memory_from_row)
                .map_err(map_sqlite_error)?;
            for row in rows {
                records.push(row.map_err(map_sqlite_error)?);
            }
        } else {
            let mut stmt = conn_guard
                .prepare(
                    "SELECT id, layer, scope, scope_id, memory_type, title, content, source_type, source_ref,
                            confidence, status, tags_json, links_json, supersedes, created_at, updated_at
                     FROM memories
                     WHERE status != 'deleted'
                     ORDER BY updated_at DESC, id DESC
                     LIMIT ?1",
                )
                .map_err(map_sqlite_error)?;
            let rows = stmt
                .query_map(params![limit as i64], memory_from_row)
                .map_err(map_sqlite_error)?;
            for row in rows {
                records.push(row.map_err(map_sqlite_error)?);
            }
        }
        Ok(records)
    }

    pub async fn search_memories(
        &self,
        options: MemorySearchOptions,
    ) -> Result<Vec<MemoryRecord>, StorageError> {
        let limit = options.limit.clamp(1, 100);
        let query = options.query.trim().to_string();
        if query.is_empty() {
            return self.list_recent_memories(options.scope, limit).await;
        }

        let terms = query
            .split_whitespace()
            .map(str::trim)
            .filter(|term| !term.is_empty())
            .take(12)
            .collect::<Vec<_>>();
        let like_patterns = if terms.is_empty() {
            vec![format!("%{}%", escape_like_pattern(&query))]
        } else {
            terms
                .iter()
                .map(|term| format!("%{}%", escape_like_pattern(term)))
                .collect::<Vec<_>>()
        };

        let conn_guard = self.lock_conn()?;
        let mut sql = String::from(
            "SELECT id, layer, scope, scope_id, memory_type, title, content, source_type, source_ref,
                    confidence, status, tags_json, links_json, supersedes, created_at, updated_at
             FROM memories
             WHERE status != 'deleted'",
        );
        let mut params_values: Vec<rusqlite::types::Value> = Vec::new();
        if let Some(scope) = options.scope {
            sql.push_str(" AND scope = ?");
            params_values.push(scope.as_str().to_string().into());
        }
        if let Some(layer) = options.layer {
            sql.push_str(" AND layer = ?");
            params_values.push(layer.as_str().to_string().into());
        }
        sql.push_str(" AND (");
        for (index, like_pattern) in like_patterns.iter().enumerate() {
            if index > 0 {
                sql.push_str(" OR ");
            }
            sql.push_str("(title LIKE ? ESCAPE '\\' OR content LIKE ? ESCAPE '\\' OR source_ref LIKE ? ESCAPE '\\')");
            params_values.push(like_pattern.clone().into());
            params_values.push(like_pattern.clone().into());
            params_values.push(like_pattern.clone().into());
        }
        sql.push_str(") ORDER BY updated_at DESC, confidence DESC, id DESC LIMIT ?");
        params_values.push((limit as i64).into());

        let mut stmt = conn_guard.prepare(&sql).map_err(map_sqlite_error)?;
        let rows = stmt
            .query_map(params_from_iter(params_values), memory_from_row)
            .map_err(map_sqlite_error)?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(map_sqlite_error)?);
        }
        Ok(records)
    }

    pub async fn list_memory_lifecycle_logs(
        &self,
        target_id: Option<i64>,
        limit: usize,
    ) -> Result<Vec<MemoryLifecycleLogRecord>, StorageError> {
        let limit = limit.clamp(1, 100);
        let conn_guard = self.lock_conn()?;
        let mut records = Vec::new();
        if let Some(target_id) = target_id {
            let mut stmt = conn_guard
                .prepare(
                    "SELECT id, action, target_type, target_id, reason, model, source_ref, created_at
                     FROM memory_lifecycle_log
                     WHERE target_type = 'memory' AND target_id = ?1
                     ORDER BY created_at DESC, id DESC
                     LIMIT ?2",
                )
                .map_err(map_sqlite_error)?;
            let rows = stmt
                .query_map(params![target_id, limit as i64], lifecycle_from_row)
                .map_err(map_sqlite_error)?;
            for row in rows {
                records.push(row.map_err(map_sqlite_error)?);
            }
        } else {
            let mut stmt = conn_guard
                .prepare(
                    "SELECT id, action, target_type, target_id, reason, model, source_ref, created_at
                     FROM memory_lifecycle_log
                     ORDER BY created_at DESC, id DESC
                     LIMIT ?1",
                )
                .map_err(map_sqlite_error)?;
            let rows = stmt
                .query_map(params![limit as i64], lifecycle_from_row)
                .map_err(map_sqlite_error)?;
            for row in rows {
                records.push(row.map_err(map_sqlite_error)?);
            }
        }
        Ok(records)
    }

    fn get_memory_event_by_id_with_conn(
        &self,
        conn: &Connection,
        id: i64,
    ) -> Result<MemoryEventRecord, StorageError> {
        conn.query_row(
            "SELECT id, scope, scope_id, event_type, content, source_ref, content_hash, created_at
             FROM memory_events WHERE id = ?1",
            params![id],
            memory_event_from_row,
        )
        .map_err(map_sqlite_error)
    }
}

fn query_memory_by_id<C: std::ops::Deref<Target = Connection>>(
    conn: &C,
    id: i64,
) -> Result<MemoryRecord, StorageError> {
    conn.query_row(
        "SELECT id, layer, scope, scope_id, memory_type, title, content, source_type, source_ref,
                confidence, status, tags_json, links_json, supersedes, created_at, updated_at
         FROM memories WHERE id = ?1",
        params![id],
        memory_from_row,
    )
    .map_err(map_sqlite_error)
}

fn memory_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryRecord> {
    let layer: String = row.get(1)?;
    let scope: String = row.get(2)?;
    let source_type: String = row.get(7)?;
    let status: String = row.get(10)?;
    let tags_json: String = row.get(11)?;
    let links_json: String = row.get(12)?;
    Ok(MemoryRecord {
        id: row.get(0)?,
        layer: layer.parse().unwrap_or_default(),
        scope: scope.parse().unwrap_or_default(),
        scope_id: row.get(3)?,
        memory_type: row.get(4)?,
        title: row.get(5)?,
        content: row.get(6)?,
        source_type: source_type.parse().unwrap_or_default(),
        source_ref: row.get(8)?,
        confidence: row.get(9)?,
        status: status.parse().unwrap_or_default(),
        tags: serde_json::from_str(&tags_json).unwrap_or_default(),
        links: serde_json::from_str(&links_json).unwrap_or_default(),
        supersedes: row.get(13)?,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
    })
}

fn memory_event_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryEventRecord> {
    let scope: String = row.get(1)?;
    Ok(MemoryEventRecord {
        id: row.get(0)?,
        scope: scope.parse().unwrap_or_default(),
        scope_id: row.get(2)?,
        event_type: row.get(3)?,
        content: row.get(4)?,
        source_ref: row.get(5)?,
        content_hash: row.get(6)?,
        created_at: row.get(7)?,
    })
}

fn lifecycle_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryLifecycleLogRecord> {
    let action: String = row.get(1)?;
    Ok(MemoryLifecycleLogRecord {
        id: row.get(0)?,
        action: action.parse().unwrap_or_default(),
        target_type: row.get(2)?,
        target_id: row.get(3)?,
        reason: row.get(4)?,
        model: row.get(5)?,
        source_ref: row.get(6)?,
        created_at: row.get(7)?,
    })
}

fn stable_text_hash(text: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

trait OptionalLikeNotFound<T> {
    fn optional_like_not_found(self) -> Result<Option<T>, StorageError>;
}

impl<T> OptionalLikeNotFound<T> for Result<T, StorageError> {
    fn optional_like_not_found(self) -> Result<Option<T>, StorageError> {
        match self {
            Ok(value) => Ok(Some(value)),
            Err(StorageError::Sqlite(rusqlite::Error::QueryReturnedNoRows)) => Ok(None),
            Err(error) => Err(error),
        }
    }
}
