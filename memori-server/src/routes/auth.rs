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
        .map(str::to_string);
    let claims = token
        .as_deref()
        .and_then(|t| decode_jwt_claims(t).ok())
        .unwrap_or_default();

    let subject = payload
        .subject
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            claims
                .get("sub")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .or_else(|| {
            claims
                .get("email")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .ok_or_else(|| ApiError::bad_request("OIDC 登录缺少 subject"))?;

    let role_from_claim = claims
        .get(&policy.auth.roles_claim)
        .and_then(extract_role_from_claims);
    let role = payload
        .role
        .as_deref()
        .map(Role::from_value)
        .or(role_from_claim)
        .unwrap_or(Role::User);

    let now = unix_now_secs();
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
                "issuer": policy.auth.issuer
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
