use super::*;

pub(crate) fn initialize_schema(conn: &Connection) -> Result<(), StorageError> {
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
             file_path UNINDEXED,
             tokenize = 'trigram'
         );
         CREATE VIRTUAL TABLE IF NOT EXISTS documents_fts USING fts5(
             search_text,
             file_name,
             relative_path,
             heading_catalog_text,
             doc_id UNINDEXED,
             file_path UNINDEXED,
             tokenize = 'trigram'
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
    migrate_legacy_fts_tables(conn)?;
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

pub(crate) fn ensure_schema_version(conn: &Connection) -> Result<(), StorageError> {
    let current: u32 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(map_sqlite_error)?;
    if current < DB_SCHEMA_VERSION {
        conn.pragma_update(None, "user_version", DB_SCHEMA_VERSION)
            .map_err(map_sqlite_error)?;
    }
    Ok(())
}

pub(crate) fn ensure_graph_task_queue_schema(conn: &Connection) -> Result<(), StorageError> {
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

pub(crate) fn ensure_documents_schema(conn: &Connection) -> Result<(), StorageError> {
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

pub(crate) fn ensure_chunks_schema(conn: &Connection) -> Result<(), StorageError> {
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

pub(crate) fn ensure_file_index_state_schema(conn: &Connection) -> Result<(), StorageError> {
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

fn migrate_legacy_fts_tables(conn: &Connection) -> Result<(), StorageError> {
    for (table, create_sql) in &[
        (
            "chunks_fts",
            "CREATE VIRTUAL TABLE chunks_fts USING fts5(
                content, heading_text, file_name, relative_path,
                chunk_id UNINDEXED, doc_id UNINDEXED, file_path UNINDEXED,
                tokenize = 'trigram'
            )",
        ),
        (
            "documents_fts",
            "CREATE VIRTUAL TABLE documents_fts USING fts5(
                search_text, file_name, relative_path, heading_catalog_text,
                doc_id UNINDEXED, file_path UNINDEXED,
                tokenize = 'trigram'
            )",
        ),
    ] {
        // Check tokenizer via FTS5 shadow config table
        let is_trigram = conn
            .query_row(
                &format!("SELECT v FROM {}_config WHERE k = 'tokenize'", table),
                [],
                |row| row.get::<_, String>(0),
            )
            .map(|v| v.contains("trigram"))
            .unwrap_or(false);

        // Use query_row instead of execute to safely probe column existence
        let has_doc_id = conn
            .query_row(&format!("SELECT doc_id FROM {} LIMIT 0", table), [], |_| {
                Ok(())
            })
            .optional()
            .is_ok();

        let has_file_name = conn
            .query_row(
                &format!("SELECT file_name FROM {} LIMIT 0", table),
                [],
                |_| Ok(()),
            )
            .optional()
            .is_ok();

        let needs_rebuild = !is_trigram || !has_doc_id || !has_file_name;

        if needs_rebuild {
            conn.execute_batch(&format!("DROP TABLE IF EXISTS {table}; {create_sql};"))
                .map_err(map_sqlite_error)?;
            // FTS data is gone — force a full rebuild so lexical index is repopulated
            set_metadata_value(conn, "rebuild_state", "required")?;
            set_metadata_value(
                conn,
                "rebuild_reason",
                "fts_schema_migration:FTS 表已升级为 trigram tokenizer，需全量重建以恢复词法索引",
            )?;
        }
    }
    Ok(())
}

fn migrate_legacy_file_catalog_ext_column(conn: &Connection) -> Result<(), StorageError> {
    let mut stmt = conn
        .prepare("PRAGMA table_info(file_catalog)")
        .map_err(map_sqlite_error)?;
    let columns: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(map_sqlite_error)?
        .filter_map(|r| r.ok())
        .collect();

    if !columns.iter().any(|c| c == "ext") {
        return Ok(());
    }

    // Build SELECT expressions defensively — old tables may be missing columns
    let col = |name: &str, default: &str| -> String {
        if columns.iter().any(|c| c == name) {
            name.to_string()
        } else {
            default.to_string()
        }
    };

    let file_ext_src = if columns.iter().any(|c| c == "file_ext") {
        "file_ext".to_string()
    } else {
        "ext".to_string()
    };

    let select = format!(
        "file_path, {}, {}, {}, {}, {}, {}, {}, {}",
        col("relative_path", "''"),
        col("parent_dir", "''"),
        col("file_name", "''"),
        file_ext_src,
        col("file_size", "0"),
        col("mtime_secs", "0"),
        col("discovered_at", "0"),
        col("removed_at", "NULL"),
    );

    conn.execute_batch(&format!(
        "BEGIN;
         CREATE TABLE file_catalog_new (
             file_path TEXT PRIMARY KEY,
             relative_path TEXT NOT NULL DEFAULT '',
             parent_dir TEXT NOT NULL DEFAULT '',
             file_name TEXT NOT NULL DEFAULT '',
             file_ext TEXT NOT NULL DEFAULT '',
             file_size INTEGER NOT NULL DEFAULT 0,
             mtime_secs INTEGER NOT NULL DEFAULT 0,
             discovered_at INTEGER NOT NULL DEFAULT 0,
             removed_at INTEGER
         );
         INSERT INTO file_catalog_new SELECT {select} FROM file_catalog;
         DROP TABLE file_catalog;
         ALTER TABLE file_catalog_new RENAME TO file_catalog;
         COMMIT;"
    ))
    .map_err(map_sqlite_error)?;

    Ok(())
}

pub(crate) fn ensure_file_catalog_schema(conn: &Connection) -> Result<(), StorageError> {
    migrate_legacy_file_catalog_ext_column(conn)?;
    ensure_table_column(
        conn,
        "file_catalog",
        "relative_path",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_table_column(
        conn,
        "file_catalog",
        "parent_dir",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_table_column(
        conn,
        "file_catalog",
        "file_name",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_table_column(conn, "file_catalog", "file_ext", "TEXT NOT NULL DEFAULT ''")?;
    ensure_table_column(
        conn,
        "file_catalog",
        "file_size",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_table_column(
        conn,
        "file_catalog",
        "mtime_secs",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_table_column(
        conn,
        "file_catalog",
        "discovered_at",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_table_column(conn, "file_catalog", "removed_at", "INTEGER")?;
    Ok(())
}

pub(crate) fn ensure_table_column(
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

pub(crate) fn ensure_system_metadata(conn: &Connection) -> Result<(), StorageError> {
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

pub(crate) fn has_existing_index_data(conn: &Connection) -> Result<bool, StorageError> {
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

pub(crate) fn get_metadata_value(
    conn: &Connection,
    key: &str,
) -> Result<Option<String>, StorageError> {
    conn.query_row(
        "SELECT value FROM system_metadata WHERE key = ?1",
        params![key],
        |row| row.get(0),
    )
    .optional()
    .map_err(map_sqlite_error)
}

pub(crate) fn set_metadata_value(
    conn: &Connection,
    key: &str,
    value: &str,
) -> Result<(), StorageError> {
    conn.execute(
        "INSERT INTO system_metadata(key, value)
         VALUES(?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map_err(map_sqlite_error)?;
    Ok(())
}

pub(crate) fn read_index_metadata_from_conn(
    conn: &Connection,
) -> Result<IndexMetadata, StorageError> {
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

pub(crate) fn current_unix_timestamp_secs() -> Result<i64, StorageError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(StorageError::Clock)?;
    i64::try_from(duration.as_secs()).map_err(|_| StorageError::TimestampOverflow)
}

pub(crate) fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
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

pub(crate) fn make_placeholders(count: usize) -> String {
    std::iter::repeat_n("?", count)
        .collect::<Vec<_>>()
        .join(",")
}

pub(crate) fn map_sqlite_error(err: rusqlite::Error) -> StorageError {
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

pub(crate) fn directory_like_pattern(dir_path: &str) -> (String, String) {
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

pub(crate) fn escape_like_pattern(text: &str) -> String {
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
