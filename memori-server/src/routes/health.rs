use crate::*;

pub(crate) async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { ok: true })
}

pub(crate) async fn admin_health_handler(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let _ = require_session(&state, &headers, Role::Operator).await?;
    let init_error_message = state.init_error.lock().await.clone();
    let engine_guard = state.engine.lock().await;
    let ready = engine_guard.is_some() && init_error_message.is_none();
    drop(engine_guard);

    let indexing_status = if ready {
        get_indexing_status_handler(State(state.clone()))
            .await
            .ok()
            .map(|r| r.0)
    } else {
        None
    };
    Ok(Json(serde_json::json!({
        "ok": ready,
        "engine_ready": ready,
        "init_error": init_error_message,
        "indexing_status": indexing_status
    })))
}
