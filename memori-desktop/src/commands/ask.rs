use super::*;

#[tauri::command]
pub(crate) async fn ask_vault_structured(
    query: String,
    lang: Option<String>,
    top_k: Option<usize>,
    scope_paths: Option<Vec<String>>,
    state: State<'_, DesktopState>,
) -> Result<AskResponseStructured, String> {
    let query = query.trim().to_string();
    info!(query = %query, lang = ?lang, top_k = ?top_k, scope_count = ?scope_paths.as_ref().map(|v| v.len()), "[用户操作] 发起搜索");
    if query.is_empty() {
        return Ok(AskResponseStructured {
            status: AskStatus::InsufficientEvidence,
            answer: String::new(),
            question: query,
            scope_paths: Vec::new(),
            citations: Vec::new(),
            evidence: Vec::new(),
            metrics: Default::default(),
            answer_source_mix: memori_core::AnswerSourceMix::Insufficient,
            memory_context: Vec::new(),
            source_groups: Vec::new(),
            failure_class: memori_core::FailureClass::RecallMiss,
            context_budget_report: memori_core::ContextBudgetReport::default(),
        });
    }

    let engine_guard = state.engine.lock().await;
    let init_error_guard = state.init_error.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_guard.as_ref() {
            return Err(format!("引擎初始化失败: {message}"));
        }
        return Err("引擎尚在初始化中，请稍后重试。".to_string());
    };

    let mut normalized_scope_paths = normalize_scope_paths(scope_paths);
    if normalized_scope_paths.is_empty()
        && let Ok(settings) = load_app_settings()
        && let Ok(watch_root) = resolve_watch_root_from_settings(&settings)
    {
        normalized_scope_paths.push(watch_root);
    }
    let scope_refs = if normalized_scope_paths.is_empty() {
        None
    } else {
        Some(normalized_scope_paths.as_slice())
    };

    let result = engine
        .ask_structured(&query, lang.as_deref(), scope_refs, top_k)
        .await
        .map_err(describe_engine_error);
    match &result {
        Ok(resp) => {
            info!(status = ?resp.status, evidence_count = resp.evidence.len(), "[用户操作] 搜索完成")
        }
        Err(e) => error!(error = %e, "[用户操作] 搜索失败"),
    }
    result
}

#[tauri::command]
pub(crate) async fn ask_vault(
    query: String,
    lang: Option<String>,
    top_k: Option<usize>,
    scope_paths: Option<Vec<String>>,
    state: State<'_, DesktopState>,
) -> Result<String, String> {
    let response = ask_vault_structured(query, lang, top_k, scope_paths, state).await?;
    Ok(format_legacy_answer(&response))
}

#[tauri::command]
pub(crate) async fn get_vault_stats(state: State<'_, DesktopState>) -> Result<VaultStats, String> {
    info!("[用户操作] 获取 Vault 统计");
    let engine_guard = state.engine.lock().await;
    let init_error_guard = state.init_error.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_guard.as_ref() {
            warn!(
                error = %message,
                "engine 尚未就绪，Vault 统计回退为直接读取 SQLite"
            );
            return get_vault_stats_from_sqlite();
        }
        return get_vault_stats_from_sqlite();
    };
    engine
        .get_vault_stats()
        .await
        .map_err(|err| err.to_string())
}

fn get_vault_stats_from_sqlite() -> Result<VaultStats, String> {
    let db_path = resolve_desktop_db_path()?;
    if !db_path.exists() {
        return Ok(VaultStats {
            document_count: 0,
            chunk_count: 0,
            graph_node_count: 0,
        });
    }

    let conn = rusqlite::Connection::open_with_flags(
        &db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
            | rusqlite::OpenFlags::SQLITE_OPEN_URI
            | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|err| format!("读取统计数据库失败({}): {err}", db_path.display()))?;

    Ok(VaultStats {
        document_count: count_table_rows(&conn, "documents")?,
        chunk_count: count_table_rows(&conn, "chunks")?,
        graph_node_count: count_table_rows(&conn, "nodes")?,
    })
}

fn resolve_desktop_db_path() -> Result<PathBuf, String> {
    if let Ok(path) = std::env::var(memori_core::MEMORI_DB_PATH_ENV) {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }
    if let Some(data_dir) = dirs::data_dir() {
        return Ok(data_dir.join("Memori-Vault").join(".memori.db"));
    }
    Ok(std::env::current_dir()
        .map_err(|err| format!("获取当前工作目录失败: {err}"))?
        .join(".memori.db"))
}

fn count_table_rows(conn: &rusqlite::Connection, table: &str) -> Result<u64, String> {
    let sql = format!("SELECT COUNT(*) FROM {table}");
    let count: i64 = conn
        .query_row(&sql, [], |row| row.get(0))
        .map_err(|err| format!("读取 {table} 统计失败: {err}"))?;
    u64::try_from(count).map_err(|_| format!("{table} 统计值异常: {count}"))
}
