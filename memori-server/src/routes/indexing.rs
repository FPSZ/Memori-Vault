use crate::*;

pub(crate) async fn admin_trigger_reindex_handler(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let session = require_session(&state, &headers, Role::Operator).await?;
    let response = trigger_reindex_handler(State(state.clone())).await?;
    append_audit_event(
        &state,
        AuditEventDto {
            actor: session.subject,
            action: "indexing.reindex".to_string(),
            resource: "indexing".to_string(),
            timestamp: unix_now_secs(),
            result: "ok".to_string(),
            metadata: serde_json::json!({}),
        },
    )
    .await;
    Ok(response)
}

pub(crate) async fn admin_pause_indexing_handler(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let session = require_session(&state, &headers, Role::Operator).await?;
    let response = pause_indexing_handler(State(state.clone())).await?;
    append_audit_event(
        &state,
        AuditEventDto {
            actor: session.subject,
            action: "indexing.pause".to_string(),
            resource: "indexing".to_string(),
            timestamp: unix_now_secs(),
            result: "ok".to_string(),
            metadata: serde_json::json!({}),
        },
    )
    .await;
    Ok(response)
}

pub(crate) async fn admin_resume_indexing_handler(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let session = require_session(&state, &headers, Role::Operator).await?;
    let response = resume_indexing_handler(State(state.clone())).await?;
    append_audit_event(
        &state,
        AuditEventDto {
            actor: session.subject,
            action: "indexing.resume".to_string(),
            resource: "indexing".to_string(),
            timestamp: unix_now_secs(),
            result: "ok".to_string(),
            metadata: serde_json::json!({}),
        },
    )
    .await;
    Ok(response)
}

pub(crate) async fn get_vault_stats_handler(
    State(state): State<ServerState>,
) -> Result<Json<VaultStats>, ApiError> {
    let init_error_message = state.init_error.lock().await.clone();
    let engine_guard = state.engine.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_message {
            return Err(ApiError::internal(format!("引擎初始化失败: {message}")));
        }
        return Err(ApiError::internal("引擎尚在初始化中，请稍后重试。"));
    };
    let stats = engine
        .get_vault_stats()
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    Ok(Json(stats))
}

pub(crate) async fn get_indexing_status_handler(
    State(state): State<ServerState>,
) -> Result<Json<IndexingStatus>, ApiError> {
    let init_error_message = state.init_error.lock().await.clone();
    let engine_guard = state.engine.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_message {
            return Err(ApiError::internal(format!("引擎初始化失败: {message}")));
        }
        return Err(ApiError::internal("引擎尚在初始化中，请稍后重试。"));
    };
    let status = engine
        .get_indexing_status()
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    Ok(Json(status))
}

pub(crate) async fn set_indexing_mode_handler(
    State(state): State<ServerState>,
    Json(payload): Json<SetIndexingModePayload>,
) -> Result<Json<AppSettingsDto>, ApiError> {
    let mut settings = load_app_settings().map_err(ApiError::internal)?;
    let mode = IndexingMode::from_value(&payload.indexing_mode);
    let budget = ResourceBudget::from_value(&payload.resource_budget);
    let schedule_window = if mode == IndexingMode::Scheduled {
        Some(ScheduleWindow {
            start: payload
                .schedule_start
                .unwrap_or_else(|| "00:00".to_string()),
            end: payload.schedule_end.unwrap_or_else(|| "06:00".to_string()),
        })
    } else {
        None
    };
    settings.indexing_mode = Some(mode.as_str().to_string());
    settings.resource_budget = Some(budget.as_str().to_string());
    settings.schedule_start = schedule_window.as_ref().map(|w| w.start.clone());
    settings.schedule_end = schedule_window.as_ref().map(|w| w.end.clone());
    save_app_settings(&settings).map_err(ApiError::internal)?;

    let init_error_message = state.init_error.lock().await.clone();
    let engine_guard = state.engine.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_message {
            return Err(ApiError::internal(format!("引擎初始化失败: {message}")));
        }
        return Err(ApiError::internal("引擎尚在初始化中，请稍后重试。"));
    };
    engine
        .set_indexing_config(IndexingConfig {
            mode,
            resource_budget: budget,
            schedule_window,
        })
        .await;

    let watch_root = resolve_watch_root_from_settings(&settings).map_err(ApiError::internal)?;
    let indexing = resolve_indexing_config(&settings);
    Ok(Json(AppSettingsDto {
        watch_root: watch_root.to_string_lossy().to_string(),
        language: settings.language,
        indexing_mode: indexing.mode.as_str().to_string(),
        resource_budget: indexing.resource_budget.as_str().to_string(),
        schedule_start: indexing.schedule_window.as_ref().map(|w| w.start.clone()),
        schedule_end: indexing.schedule_window.as_ref().map(|w| w.end.clone()),
    }))
}

pub(crate) async fn trigger_reindex_handler(
    State(state): State<ServerState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let init_error_message = state.init_error.lock().await.clone();
    let engine_guard = state.engine.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_message {
            return Err(ApiError::internal(format!("引擎初始化失败: {message}")));
        }
        return Err(ApiError::internal("引擎尚在初始化中，请稍后重试。"));
    };
    engine
        .trigger_reindex()
        .await
        .map_err(map_engine_api_error)?;
    Ok(Json(serde_json::json!({
        "task_id": format!("reindex-{}", chrono_like_now_token())
    })))
}

pub(crate) async fn pause_indexing_handler(
    State(state): State<ServerState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let init_error_message = state.init_error.lock().await.clone();
    let engine_guard = state.engine.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_message {
            return Err(ApiError::internal(format!("引擎初始化失败: {message}")));
        }
        return Err(ApiError::internal("引擎尚在初始化中，请稍后重试。"));
    };
    engine.pause_indexing().await;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn resume_indexing_handler(
    State(state): State<ServerState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let init_error_message = state.init_error.lock().await.clone();
    let engine_guard = state.engine.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_message {
            return Err(ApiError::internal(format!("引擎初始化失败: {message}")));
        }
        return Err(ApiError::internal("引擎尚在初始化中，请稍后重试。"));
    };
    engine.resume_indexing().await;
    Ok(Json(serde_json::json!({ "ok": true })))
}
