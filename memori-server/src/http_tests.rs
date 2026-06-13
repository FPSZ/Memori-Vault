//! HTTP 层端到端集成测试：经真实 axum router 走中间件链，验证 request-id 头部、
//! 限流 429、OpenAPI 端点。补齐审计 E6/E5 仅有纯函数单测、缺端到端覆盖的缺口。

use std::collections::HashMap;
use std::sync::Arc;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use tokio::sync::Mutex;
use tower::ServiceExt;

use crate::{RateLimiter, ServerMetrics, ServerState, build_router};

/// 构造不含真实引擎的最小 ServerState（健康/openapi/限流路径不需要引擎）。
fn test_state(rate_limiter: RateLimiter) -> ServerState {
    ServerState {
        engine: Arc::new(Mutex::new(None)),
        init_error: Arc::new(Mutex::new(None)),
        sessions: Arc::new(Mutex::new(HashMap::new())),
        metrics: Arc::new(ServerMetrics::default()),
        audit_file_lock: Arc::new(Mutex::new(())),
        rate_limiter: Arc::new(rate_limiter),
    }
}

#[tokio::test]
async fn health_returns_ok_with_generated_request_id() {
    let app = build_router(test_state(RateLimiter::new(true, 600, 20)));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let request_id = response
        .headers()
        .get("x-request-id")
        .expect("x-request-id present")
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        !request_id.is_empty(),
        "generated request id should be non-empty"
    );
}

#[tokio::test]
async fn request_id_is_propagated_from_client() {
    let app = build_router(test_state(RateLimiter::new(true, 600, 20)));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .header("x-request-id", "client-trace-xyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.headers().get("x-request-id").unwrap(),
        "client-trace-xyz"
    );
}

#[tokio::test]
async fn openapi_endpoint_serves_valid_spec() {
    let app = build_router(test_state(RateLimiter::new(true, 600, 20)));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let spec: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(spec["openapi"], "3.1.0");
    assert!(spec["paths"]["/api/ask"]["post"].is_object());
}

#[tokio::test]
async fn sensitive_endpoint_rate_limited_after_threshold() {
    // 敏感桶上限 2：前 2 个放行（handler 因无 token 返回 401），第 3 个被限流 429。
    let app = build_router(test_state(RateLimiter::new(true, 600, 2)));
    let mut statuses = Vec::new();
    for _ in 0..3 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/auth/logout")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        statuses.push(response.status());
    }
    assert_eq!(statuses[0], StatusCode::UNAUTHORIZED);
    assert_eq!(statuses[1], StatusCode::UNAUTHORIZED);
    assert_eq!(statuses[2], StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn rate_limit_disabled_lets_all_through() {
    let app = build_router(test_state(RateLimiter::new(false, 600, 1)));
    for _ in 0..5 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/auth/logout")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // 限流关闭：永不 429（无 token 仍是 401）。
        assert_ne!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }
}
