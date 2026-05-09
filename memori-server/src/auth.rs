use base64::Engine;

use crate::*;

pub(crate) async fn require_session(
    state: &ServerState,
    headers: &HeaderMap,
    minimum_role: Role,
) -> Result<SessionInfo, ApiError> {
    let Some(session_token) = extract_bearer_token(headers) else {
        return Err(ApiError::unauthorized(
            "missing bearer token, please log in first",
        ));
    };
    let now = unix_now_secs();
    let mut sessions = state.sessions.lock().await;
    sessions.retain(|_, session| session.expires_at > now);
    let Some(session) = sessions.get(&session_token).cloned() else {
        return Err(ApiError::unauthorized(
            "session expired, please log in again",
        ));
    };
    if session.role < minimum_role {
        return Err(ApiError::forbidden("insufficient permissions"));
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
    let header = parts
        .next()
        .ok_or_else(|| "invalid JWT: missing header".to_string())?;
    let payload = parts
        .next()
        .ok_or_else(|| "invalid JWT: missing payload".to_string())?;
    let signature = parts
        .next()
        .ok_or_else(|| "invalid JWT: missing signature".to_string())?;
    if parts.next().is_some() {
        return Err("invalid JWT: expected exactly 3 segments".to_string());
    }
    if signature.trim().is_empty() {
        return Err("invalid JWT: empty signature".to_string());
    }

    let header_json = decode_jwt_segment(header, "header")?;
    if header_json
        .get("alg")
        .and_then(|value| value.as_str())
        .is_some_and(|alg| alg.eq_ignore_ascii_case("none"))
    {
        return Err("invalid JWT: alg=none is not allowed".to_string());
    }

    decode_jwt_segment(payload, "payload")
}

fn decode_jwt_segment(segment: &str, segment_name: &str) -> Result<serde_json::Value, String> {
    let mut text = segment.trim().to_string();
    if text.is_empty() {
        return Err(format!("invalid JWT: empty {segment_name}"));
    }
    while !text.len().is_multiple_of(4) {
        text.push('=');
    }
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(text.as_bytes())
        .map_err(|err| format!("invalid JWT: failed to decode {segment_name}: {err}"))?;
    serde_json::from_slice(&decoded)
        .map_err(|err| format!("invalid JWT: failed to parse {segment_name}: {err}"))
}

pub(crate) fn validate_oidc_claims(
    claims: &serde_json::Value,
    issuer: &str,
    audience: &str,
    now: i64,
) -> Result<(), String> {
    let expected_issuer = issuer.trim();
    if !expected_issuer.is_empty() {
        let actual_issuer = claims
            .get("iss")
            .and_then(|value| value.as_str())
            .ok_or_else(|| "OIDC token is missing issuer".to_string())?;
        if actual_issuer.trim() != expected_issuer {
            return Err("OIDC token issuer does not match policy".to_string());
        }
    }

    let expected_audience = audience.trim();
    if !expected_audience.is_empty() && !claim_matches_audience(claims.get("aud"), expected_audience)
    {
        return Err("OIDC token audience does not match policy".to_string());
    }

    if let Some(exp) = claims.get("exp").and_then(json_i64) && exp <= now {
        return Err("OIDC token has expired".to_string());
    }

    if let Some(nbf) = claims.get("nbf").and_then(json_i64) && nbf > now {
        return Err("OIDC token is not active yet".to_string());
    }

    Ok(())
}

fn claim_matches_audience(value: Option<&serde_json::Value>, expected: &str) -> bool {
    match value {
        Some(serde_json::Value::String(text)) => text.trim() == expected,
        Some(serde_json::Value::Array(values)) => values
            .iter()
            .filter_map(|item| item.as_str())
            .any(|item| item.trim() == expected),
        _ => false,
    }
}

fn json_i64(value: &serde_json::Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_jwt_claims_rejects_alg_none() {
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(r#"{"alg":"none"}"#);
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(r#"{"sub":"demo"}"#);
        let token = format!("{header}.{payload}.sig");
        let err = decode_jwt_claims(&token).unwrap_err();
        assert!(err.contains("invalid JWT"));
    }

    #[test]
    fn validate_oidc_claims_accepts_audience_array() {
        let claims = serde_json::json!({
            "iss": "https://issuer.test",
            "aud": ["memori-client", "other"],
            "exp": unix_now_secs() + 60,
        });
        validate_oidc_claims(&claims, "https://issuer.test", "memori-client", unix_now_secs())
            .unwrap();
    }
}
