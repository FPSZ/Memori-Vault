use super::*;

#[tauri::command]
pub(crate) async fn get_indexing_status(
    state: State<'_, DesktopState>,
) -> Result<IndexingStatus, String> {
    info!("[用户操作] 获取索引状态");
    let engine_guard = state.engine.lock().await;
    let init_error_guard = state.init_error.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_guard.as_ref() {
            if is_model_not_configured_message(message) {
                let settings = load_app_settings()?;
                return Ok(default_indexing_status(&settings));
            }
            return Err(format!("引擎初始化失败: {message}"));
        }
        let settings = load_app_settings()?;
        return Ok(default_indexing_status(&settings));
    };
    engine
        .get_indexing_status()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub(crate) async fn set_indexing_mode(
    payload: SetIndexingModePayload,
    state: State<'_, DesktopState>,
) -> Result<AppSettingsDto, String> {
    info!(mode = %payload.indexing_mode, budget = %payload.resource_budget, "[用户操作] 修改索引配置");
    let mut settings = load_app_settings()?;
    let mode = IndexingMode::from_value(&payload.indexing_mode);
    let budget = ResourceBudget::from_value(&payload.resource_budget);
    let schedule_window = if mode == IndexingMode::Scheduled {
        let start = payload
            .schedule_start
            .unwrap_or_else(|| "00:00".to_string())
            .trim()
            .to_string();
        let end = payload
            .schedule_end
            .unwrap_or_else(|| "06:00".to_string())
            .trim()
            .to_string();
        Some(ScheduleWindow { start, end })
    } else {
        None
    };
    settings.indexing_mode = Some(mode.as_str().to_string());
    settings.resource_budget = Some(budget.as_str().to_string());
    settings.schedule_start = schedule_window.as_ref().map(|w| w.start.clone());
    settings.schedule_end = schedule_window.as_ref().map(|w| w.end.clone());
    save_app_settings(&settings)?;

    let engine_guard = state.engine.lock().await;
    let init_error_guard = state.init_error.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_guard.as_ref() {
            return Err(format!("引擎初始化失败: {message}"));
        }
        return Err("引擎尚在初始化中，请稍后重试。".to_string());
    };

    engine
        .set_indexing_config(IndexingConfig {
            mode,
            resource_budget: budget,
            schedule_window,
        })
        .await;

    let watch_root = resolve_watch_root_from_settings(&settings)?;
    let indexing = resolve_indexing_config(&settings);
    Ok(AppSettingsDto::from_settings(
        settings,
        watch_root.to_string_lossy().to_string(),
        indexing,
    ))
}

#[tauri::command]
pub(crate) async fn trigger_reindex(state: State<'_, DesktopState>) -> Result<String, String> {
    info!("[用户操作] 触发重建索引");
    let task_id = format!("reindex-{}", chrono_like_now_token());
    let engine_guard = state.engine.lock().await;
    let init_error_guard = state.init_error.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_guard.as_ref() {
            return Err(format!("引擎初始化失败: {message}"));
        }
        return Err("引擎尚在初始化中，请稍后重试。".to_string());
    };
    engine
        .trigger_reindex()
        .await
        .map_err(describe_engine_error)?;
    Ok(task_id)
}

#[tauri::command]
pub(crate) async fn pause_indexing(state: State<'_, DesktopState>) -> Result<(), String> {
    info!("[用户操作] 暂停索引");
    let engine_guard = state.engine.lock().await;
    let init_error_guard = state.init_error.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_guard.as_ref() {
            return Err(format!("引擎初始化失败: {message}"));
        }
        return Err("引擎尚在初始化中，请稍后重试。".to_string());
    };
    engine.pause_indexing().await;
    Ok(())
}

#[tauri::command]
pub(crate) async fn resume_indexing(state: State<'_, DesktopState>) -> Result<(), String> {
    info!("[用户操作] 恢复索引");
    let engine_guard = state.engine.lock().await;
    let init_error_guard = state.init_error.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_guard.as_ref() {
            return Err(format!("引擎初始化失败: {message}"));
        }
        return Err("引擎尚在初始化中，请稍后重试。".to_string());
    };
    engine.resume_indexing().await;
    Ok(())
}

#[tauri::command]
pub(crate) async fn get_index_filter() -> Result<Option<IndexFilterConfig>, String> {
    let settings = load_app_settings()?;
    Ok(settings.index_filter)
}

#[tauri::command]
pub(crate) async fn set_index_filter(
    payload: IndexFilterConfig,
    state: State<'_, DesktopState>,
) -> Result<AppSettingsDto, String> {
    info!(enabled = payload.enabled, "[用户操作] 修改索引筛选配置");
    let mut settings = load_app_settings()?;
    settings.index_filter = Some(payload.clone());
    save_app_settings(&settings)?;

    let engine_guard = state.engine.lock().await;
    let init_error_guard = state.init_error.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_guard.as_ref() {
            return Err(format!("引擎初始化失败: {message}"));
        }
        return Err("引擎尚在初始化中，请稍后重试。".to_string());
    };

    let core_filter = if payload.enabled {
        Some(memori_core::IndexFilterConfig {
            enabled: payload.enabled,
            include_extensions: payload.include_extensions,
            exclude_extensions: payload.exclude_extensions,
            exclude_paths: payload.exclude_paths,
            include_paths: payload.include_paths,
            min_mtime: payload.min_mtime,
            max_mtime: payload.max_mtime,
            min_size: payload.min_size,
            max_size: payload.max_size,
        })
    } else {
        None
    };
    engine.set_index_filter_config(core_filter).await;

    let watch_root = resolve_watch_root_from_settings(&settings)?;
    let indexing = resolve_indexing_config(&settings);
    Ok(AppSettingsDto::from_settings(
        settings,
        watch_root.to_string_lossy().to_string(),
        indexing,
    ))
}
