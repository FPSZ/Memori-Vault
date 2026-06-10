use base64::Engine;
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;

use crate::*;

const OIDC_HTTP_TIMEOUT_SECS: u64 = 10;
const OIDC_CLOCK_SKEW_SECS: i64 = 300;

#[derive(Debug, Deserialize)]
struct OidcDiscoveryDocument {
    jwks_uri: String,
}

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
    if is_configured_admin_token(&session_token) {
        let now = unix_now_secs();
        return Ok(SessionInfo {
            subject: "server-admin-token".to_string(),
            role: Role::Admin,
            issued_at: now,
            expires_at: now + DEFAULT_SESSION_TTL_SECS,
        });
    }
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

/// 先清掉已过期会话，再按上限淘汰最早签发的，使插入新会话后总数不超过 `cap`。
/// 抽成纯函数便于单测；调用方持有 sessions 锁。
pub(crate) fn enforce_session_cap(
    sessions: &mut std::collections::HashMap<String, SessionInfo>,
    now: i64,
    cap: usize,
) {
    sessions.retain(|_, session| session.expires_at > now);
    while sessions.len() >= cap {
        let Some(oldest_token) = sessions
            .iter()
            .min_by_key(|(_, session)| session.issued_at)
            .map(|(token, _)| token.clone())
        else {
            break;
        };
        sessions.remove(&oldest_token);
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

pub(crate) fn allow_insecure_oidc_dev_login() -> bool {
    std::env::var("MEMORI_INSECURE_OIDC_DEV_LOGIN")
        .ok()
        .is_some_and(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
}

pub(crate) async fn verify_oidc_token_claims(
    token: &str,
    issuer: &str,
    audience: &str,
) -> Result<serde_json::Value, String> {
    let issuer = issuer.trim().trim_end_matches('/');
    let audience = audience.trim();
    if issuer.is_empty() {
        return Err("OIDC issuer is not configured".to_string());
    }
    if audience.is_empty() {
        return Err("OIDC client_id is not configured".to_string());
    }

    let header = decode_header(token).map_err(|err| format!("invalid JWT header: {err}"))?;
    let alg = header.alg;
    if !is_allowed_oidc_alg(alg) {
        return Err(format!("OIDC token alg is not allowed: {alg:?}"));
    }
    let kid = header
        .kid
        .as_deref()
        .map(str::trim)
        .filter(|kid| !kid.is_empty())
        .ok_or_else(|| "OIDC token header is missing kid".to_string())?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(OIDC_HTTP_TIMEOUT_SECS))
        .build()
        .map_err(|err| format!("failed to build OIDC HTTP client: {err}"))?;
    let discovery_url = format!("{issuer}/.well-known/openid-configuration");
    let discovery = client
        .get(&discovery_url)
        .send()
        .await
        .map_err(|err| format!("failed to fetch OIDC discovery document: {err}"))?
        .error_for_status()
        .map_err(|err| format!("OIDC discovery request failed: {err}"))?
        .json::<OidcDiscoveryDocument>()
        .await
        .map_err(|err| format!("failed to parse OIDC discovery document: {err}"))?;
    let jwks = client
        .get(discovery.jwks_uri)
        .send()
        .await
        .map_err(|err| format!("failed to fetch OIDC JWKS: {err}"))?
        .error_for_status()
        .map_err(|err| format!("OIDC JWKS request failed: {err}"))?
        .json::<JwkSet>()
        .await
        .map_err(|err| format!("failed to parse OIDC JWKS: {err}"))?;
    let jwk = jwks
        .keys
        .iter()
        .find(|key| key.common.key_id.as_deref() == Some(kid))
        .ok_or_else(|| "OIDC JWKS does not contain token kid".to_string())?;
    let decoding_key =
        DecodingKey::from_jwk(jwk).map_err(|err| format!("invalid OIDC JWK: {err}"))?;

    let mut validation = Validation::new(alg);
    validation.validate_aud = false;
    validation.validate_exp = false;
    validation.validate_nbf = false;
    validation.required_spec_claims.clear();
    let claims = decode::<serde_json::Value>(token, &decoding_key, &validation)
        .map_err(|err| format!("OIDC token signature verification failed: {err}"))?
        .claims;

    validate_oidc_claims(&claims, issuer, audience, unix_now_secs())?;
    Ok(claims)
}

fn is_allowed_oidc_alg(alg: Algorithm) -> bool {
    matches!(
        alg,
        Algorithm::RS256
            | Algorithm::RS384
            | Algorithm::RS512
            | Algorithm::PS256
            | Algorithm::PS384
            | Algorithm::PS512
            | Algorithm::ES256
            | Algorithm::ES384
    )
}

fn is_configured_admin_token(token: &str) -> bool {
    let configured = std::env::var("MEMORI_SERVER_ADMIN_TOKEN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| value.len() >= 24);
    configured
        .as_deref()
        .is_some_and(|expected| constant_time_eq(token.trim().as_bytes(), expected.as_bytes()))
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in left.iter().zip(right.iter()) {
        diff |= a ^ b;
    }
    diff == 0
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
    if !expected_audience.is_empty()
        && !claim_matches_audience(claims.get("aud"), expected_audience)
    {
        return Err("OIDC token audience does not match policy".to_string());
    }

    let exp = claims
        .get("exp")
        .and_then(json_i64)
        .ok_or_else(|| "OIDC token is missing expiration".to_string())?;
    if exp <= now {
        return Err("OIDC token has expired".to_string());
    }

    if let Some(nbf) = claims.get("nbf").and_then(json_i64)
        && nbf > now + OIDC_CLOCK_SKEW_SECS
    {
        return Err("OIDC token is not active yet".to_string());
    }

    let iat = claims
        .get("iat")
        .and_then(json_i64)
        .ok_or_else(|| "OIDC token is missing issued-at".to_string())?;
    if iat > now + OIDC_CLOCK_SKEW_SECS {
        return Err("OIDC token issued-at is in the future".to_string());
    }

    if let Some(azp) = claims.get("azp").and_then(|value| value.as_str())
        && !expected_audience.is_empty()
        && azp.trim() != expected_audience
    {
        return Err("OIDC token authorized party does not match policy".to_string());
    }
    if matches!(claims.get("aud"), Some(serde_json::Value::Array(values)) if values.len() > 1)
        && claims
            .get("azp")
            .and_then(|value| value.as_str())
            .map(str::trim)
            != Some(expected_audience)
    {
        return Err("OIDC token with multiple audiences must include matching azp".to_string());
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
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"alg":"none"}"#);
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"sub":"demo"}"#);
        let token = format!("{header}.{payload}.sig");
        let err = decode_jwt_claims(&token).unwrap_err();
        assert!(err.contains("invalid JWT"));
    }

    #[test]
    fn validate_oidc_claims_accepts_audience_array() {
        let claims = serde_json::json!({
            "iss": "https://issuer.test",
            "aud": ["memori-client", "other"],
            "azp": "memori-client",
            "exp": unix_now_secs() + 60,
            "iat": unix_now_secs(),
        });
        validate_oidc_claims(
            &claims,
            "https://issuer.test",
            "memori-client",
            unix_now_secs(),
        )
        .unwrap();
    }

    #[test]
    fn insecure_oidc_dev_login_defaults_to_disabled() {
        unsafe {
            std::env::remove_var("MEMORI_INSECURE_OIDC_DEV_LOGIN");
        }
        assert!(!allow_insecure_oidc_dev_login());
    }

    fn session_at(issued_at: i64, expires_at: i64) -> SessionInfo {
        SessionInfo {
            subject: format!("s{issued_at}"),
            role: Role::Viewer,
            issued_at,
            expires_at,
        }
    }

    #[test]
    fn enforce_session_cap_drops_expired_then_oldest() {
        let now = 1_000;
        let mut sessions = std::collections::HashMap::new();
        sessions.insert("expired".to_string(), session_at(100, now - 1)); // 已过期
        sessions.insert("old".to_string(), session_at(200, now + 10_000));
        sessions.insert("mid".to_string(), session_at(300, now + 10_000));
        sessions.insert("new".to_string(), session_at(400, now + 10_000));

        // cap=3：先删过期(expired)→剩 3 个 == cap，再淘汰最早(old)→剩 2，给新插入留位。
        enforce_session_cap(&mut sessions, now, 3);

        assert!(!sessions.contains_key("expired"), "过期会话应被清掉");
        assert!(!sessions.contains_key("old"), "达上限应淘汰最早签发的");
        assert!(sessions.contains_key("mid"));
        assert!(sessions.contains_key("new"));
        assert!(sessions.len() < 3, "淘汰后应低于 cap，为新会话留位");
    }
}
