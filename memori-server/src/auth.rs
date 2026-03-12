use crate::*;

pub(crate) async fn require_session(
    state: &ServerState,
    headers: &HeaderMap,
    minimum_role: Role,
) -> Result<SessionInfo, ApiError> {
    let Some(session_token) = extract_bearer_token(headers) else {
        return Err(ApiError::unauthorized("缺少认证信息，请先登录"));
    };
    let now = unix_now_secs();
    let mut sessions = state.sessions.lock().await;
    sessions.retain(|_, session| session.expires_at > now);
    let Some(session) = sessions.get(&session_token).cloned() else {
        return Err(ApiError::unauthorized("会话已失效，请重新登录"));
    };
    if session.role < minimum_role {
        return Err(ApiError::forbidden("当前账号权限不足"));
    }
    Ok(session)
}

pub(crate) async fn resolve_actor_subject(state: &ServerState, headers: &HeaderMap) -> String {
    match require_session(state, headers, Role::Viewer).await {
        Ok(session) => session.subject,
        Err(_) => "anonymous".to_string(),
    }
}

pub(crate) fn decode_jwt_claims(token: &str) -> Result<serde_json::Value, String> {
    let mut parts = token.split('.');
    let _header = parts.next();
    let payload = parts
        .next()
        .ok_or_else(|| "JWT 结构非法：缺少 payload".to_string())?;
    let mut payload_text = payload.trim().to_string();
    if payload_text.is_empty() {
        return Err("JWT payload 为空".to_string());
    }
    while payload_text.len() % 4 != 0 {
        payload_text.push('=');
    }
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_text.as_bytes())
        .map_err(|err| format!("JWT payload 解码失败: {err}"))?;
    serde_json::from_slice(&decoded).map_err(|err| format!("JWT payload 解析失败: {err}"))
}

pub(crate) fn extract_role_from_claims(value: &serde_json::Value) -> Option<Role> {
    match value {
        serde_json::Value::String(role) => Some(Role::from_value(role)),
        serde_json::Value::Array(items) => items
            .iter()
            .filter_map(|item| item.as_str().map(Role::from_value))
            .max(),
        _ => None,
    }
}

pub(crate) fn unix_now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub(crate) fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    let value = headers.get("authorization")?;
    let text = value.to_str().ok()?.trim();
    let token = text
        .strip_prefix("Bearer ")
        .or_else(|| text.strip_prefix("bearer "))?;
    let trimmed = token.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
