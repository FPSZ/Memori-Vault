use super::*;

pub(crate) fn resolve_db_path() -> Result<PathBuf, EngineError> {
    if let Ok(path) = std::env::var(MEMORI_DB_PATH_ENV) {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }

    if let Some(data_dir) = dirs::data_dir() {
        // Stable per-user location for desktop/server deployments.
        // Example (Windows): %APPDATA%/Memori-Vault/.memori.db
        // Example (Linux): ~/.local/share/Memori-Vault/.memori.db
        return Ok(data_dir.join("Memori-Vault").join(DEFAULT_DB_FILE_NAME));
    }

    Ok(std::env::current_dir()
        .map_err(EngineError::CurrentDir)?
        .join(DEFAULT_DB_FILE_NAME))
}

/// 提供给外部壳层（如 Tauri IPC）的答案合成入口。
pub async fn generate_answer_with_context(
    question: &str,
    text_context: &str,
    graph_context: &str,
) -> Result<String, EngineError> {
    generate_llm_answer(question, text_context, graph_context).await
}
