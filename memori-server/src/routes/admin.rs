use crate::*;

pub(crate) async fn admin_metrics_handler(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<Json<ServerMetricsDto>, ApiError> {
    let _ = require_session(&state, &headers, Role::Operator).await?;
    Ok(Json(snapshot_metrics(&state.metrics)))
}

pub(crate) async fn get_enterprise_policy_handler(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<Json<EnterprisePolicyDto>, ApiError> {
    let _ = require_session(&state, &headers, Role::Operator).await?;
    let settings = load_app_settings().map_err(ApiError::internal)?;
    Ok(Json(resolve_enterprise_policy(&settings)))
}

pub(crate) async fn update_enterprise_policy_handler(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(payload): Json<EnterprisePolicyDto>,
) -> Result<Json<EnterprisePolicyDto>, ApiError> {
    let session = require_session(&state, &headers, Role::Admin).await?;
    let mut settings = load_app_settings().map_err(ApiError::internal)?;
    settings.enterprise_egress_mode = Some(
        match payload.egress_mode {
            EgressMode::LocalOnly => "local_only",
            EgressMode::Allowlist => "allowlist",
        }
        .to_string(),
    );
    settings.enterprise_allowed_model_endpoints = Some(
        payload
            .allowed_model_endpoints
            .iter()
            .map(|item| item.trim().to_ascii_lowercase())
            .filter(|item| !item.is_empty())
            .collect(),
    );
    settings.enterprise_allowed_models = Some(
        payload
            .allowed_models
            .iter()
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect(),
    );
    settings.oidc_issuer = normalize_optional_text(Some(payload.auth.issuer));
    settings.oidc_client_id = normalize_optional_text(Some(payload.auth.client_id));
    settings.oidc_redirect_uri = normalize_optional_text(Some(payload.auth.redirect_uri));
    settings.oidc_roles_claim = normalize_optional_text(Some(payload.auth.roles_claim));
    settings.indexing_mode = Some(payload.indexing_default_mode.clone());
    settings.resource_budget = Some(payload.resource_budget_default.clone());
    save_app_settings(&settings).map_err(ApiError::internal)?;
    let watch_root = resolve_watch_root_from_settings(&settings).map_err(ApiError::internal)?;
    replace_engine(
        &state.engine,
        &state.init_error,
        watch_root,
        "policy_update",
    )
    .await
    .map_err(ApiError::internal)?;

    let policy = resolve_enterprise_policy(&settings);
    append_audit_event(
        &state,
        AuditEventDto {
            actor: session.subject,
            action: "policy.update".to_string(),
            resource: "enterprise_policy".to_string(),
            timestamp: unix_now_secs(),
            result: "ok".to_string(),
            metadata: serde_json::json!({
                "egress_mode": policy.egress_mode,
                "allowed_model_endpoints": policy.allowed_model_endpoints.len(),
                "allowed_models": policy.allowed_models.len()
            }),
        },
    )
    .await;

    Ok(Json(policy))
}

pub(crate) async fn get_audit_events_handler(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Query(query): Query<AuditQuery>,
) -> Result<Json<AuditListResponse>, ApiError> {
    let _ = require_session(&state, &headers, Role::Operator).await?;
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(50).clamp(1, 200);
    let all_events = read_audit_events().map_err(ApiError::internal)?;
    let total = all_events.len();
    let start = (page - 1) * page_size;
    let items = if start >= total {
        Vec::new()
    } else {
        all_events
            .into_iter()
            .skip(start)
            .take(page_size)
            .collect::<Vec<_>>()
    };
    Ok(Json(AuditListResponse {
        total,
        page,
        page_size,
        items,
    }))
}
