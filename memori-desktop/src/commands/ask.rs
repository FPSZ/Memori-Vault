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
    if query.is_empty() {
        return Ok(AskResponseStructured {
            status: AskStatus::InsufficientEvidence,
            answer: String::new(),
            question: query,
            scope_paths: Vec::new(),
            citations: Vec::new(),
            evidence: Vec::new(),
            metrics: Default::default(),
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

    engine
        .ask_structured(&query, lang.as_deref(), scope_refs, top_k)
        .await
        .map_err(describe_engine_error)
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
    let engine_guard = state.engine.lock().await;
    let init_error_guard = state.init_error.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_guard.as_ref() {
            if is_model_not_configured_message(message) {
                return Ok(VaultStats {
                    document_count: 0,
                    chunk_count: 0,
                    graph_node_count: 0,
                });
            }
            return Err(format!("引擎初始化失败: {message}"));
        }
        return Ok(VaultStats {
            document_count: 0,
            chunk_count: 0,
            graph_node_count: 0,
        });
    };
    engine
        .get_vault_stats()
        .await
        .map_err(|err| err.to_string())
}
