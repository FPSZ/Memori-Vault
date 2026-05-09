use crate::*;

pub(crate) async fn oidc_login_handler(
    State(state): State<ServerState>,
    Json(payload): Json<OidcLoginRequest>,
) -> Result<Json<OidcLoginResponse>, ApiError> {
    let policy = resolve_enterprise_policy(&load_app_settings().map_err(ApiError::internal)?);
    let token = payload
        .id_token
        .as_deref()
        .or(payload.access_token.as_deref())
        .ok_or_else(|| ApiError::bad_request("OIDC login requires id_token or access_token"))?;
    let claims = decode_jwt_claims(token).map_err(ApiError::unauthorized)?;
    let now = unix_now_secs();
    validate_oidc_claims(
        &claims,
        &policy.auth.issuer,
        &policy.auth.client_id,
        now,
    )
    .map_err(ApiError::unauthorized)?;

    let subject = claims
        .get("sub")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            claims
                .get("email")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
        })
        .map(ToOwned::to_owned)
        .ok_or_else(|| ApiError::bad_request("OIDC token is missing subject"))?;

    let role = claims
        .get(&policy.auth.roles_claim)
        .and_then(extract_role_from_claims)
        .unwrap_or(Role::User);

    let expires_at = now + DEFAULT_SESSION_TTL_SECS;
    let session_token = Uuid::new_v4().to_string();
    {
        let mut sessions = state.sessions.lock().await;
        sessions.insert(
            session_token.clone(),
            SessionInfo {
                subject: subject.clone(),
                role,
                issued_at: now,
                expires_at,
            },
        );
    }

    append_audit_event(
        &state,
        AuditEventDto {
            actor: subject.clone(),
            action: "auth.login".to_string(),
            resource: "oidc".to_string(),
            timestamp: now,
            result: "ok".to_string(),
            metadata: serde_json::json!({
                "role": role,
                "issuer": claims.get("iss").and_then(|value| value.as_str()).unwrap_or(policy.auth.issuer.as_str())
            }),
        },
    )
    .await;

    Ok(Json(OidcLoginResponse {
        session_token,
        subject,
        role,
        expires_at,
    }))
}

pub(crate) async fn auth_me_handler(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<Json<SessionDto>, ApiError> {
    let session = require_session(&state, &headers, Role::Viewer).await?;
    Ok(Json(SessionDto {
        subject: session.subject,
        role: session.role,
        issued_at: session.issued_at,
        expires_at: session.expires_at,
    }))
}
