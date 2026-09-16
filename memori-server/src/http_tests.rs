//! HTTP 层端到端集成测试：经真实 axum router 走中间件链，验证 request-id 头部、
//! 限流 429、OpenAPI 端点。补齐审计 E6/E5 仅有纯函数单测、缺端到端覆盖的缺口。

use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::Extension;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use tokio::sync::Mutex;
use tower::ServiceExt;

use crate::{
    RateLimiter, RequestId, ServerMetrics, ServerState, build_router, request_id_trace_middleware,
};

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
async fn request_id_is_injected_into_extensions() {
    // 独立小路由验证中间件把 request-id 注入 extensions（handler 读到的值 = 请求头值），
    // 这是 ask 审计写入 request_id 的机制基础。
    let app = Router::new()
        .route(
            "/echo-request-id",
            get(|Extension(request_id): Extension<RequestId>| async move { request_id.0 }),
        )
        .layer(axum::middleware::from_fn(request_id_trace_middleware));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/echo-request-id")
                .header("x-request-id", "client-trace-xyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&body[..], b"client-trace-xyz");
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
async fn swagger_docs_page_serves_html_and_assets() {
    let app = build_router(test_state(RateLimiter::new(true, 600, 20)));
    // 文档页：200 + HTML 内容指向 openapi.json。
    let page = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/docs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(page.status(), StatusCode::OK);
    let page_body = to_bytes(page.into_body(), usize::MAX).await.unwrap();
    let page_text = String::from_utf8_lossy(&page_body);
    assert!(
        page_text.contains("swagger-ui"),
        "页面应包含 swagger-ui 挂载点"
    );
    assert!(
        page_text.contains("/api/openapi.json"),
        "页面数据源应指向 openapi.json"
    );
    // CSS 与 JS 静态资源：200 + 正确 content-type + 非空。
    for (uri, expected_type) in [
        ("/api/docs/swagger-ui.css", "text/css"),
        ("/api/docs/swagger-ui-bundle.js", "application/javascript"),
        (
            "/api/docs/swagger-ui-standalone-preset.js",
            "application/javascript",
        ),
    ] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "uri {uri}");
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert!(
            content_type.starts_with(expected_type),
            "uri {uri} type {content_type}"
        );
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert!(body.len() > 1000, "uri {uri} 资源应非空");
    }
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
