use super::*;

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

    if a.len() != b.len() {
        // 维度不匹配意味着索引中混入了不同 embedding 模型的向量，
        // 必须重建索引后才能切换模型。此处 panic 防止静默错误分数污染排序。
        panic!(
            "cosine_similarity dimension mismatch: query={}, stored={}. \
             Rebuild index after switching embedding models.",
            a.len(),
            b.len()
        );
    }

    let len = a.len();
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
