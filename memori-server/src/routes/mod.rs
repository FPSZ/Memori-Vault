use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{HeaderValue, Method};

use crate::*;

mod admin;
mod ask;
mod auth;
mod health;
mod indexing;
mod models;
mod openapi;
mod settings;

pub(crate) use admin::*;
pub(crate) use ask::*;
pub(crate) use auth::*;
pub(crate) use health::*;
pub(crate) use indexing::*;
pub(crate) use models::*;
pub(crate) use openapi::*;
pub(crate) use settings::*;

/// build_router 注册的、被 OpenAPI 路由表（openapi.rs `ROUTES`）覆盖的 REST 方法数。
/// 不含 `/api/openapi.json` 自身。改路由须同步此常量与 `ROUTES`，否则单测失败。
#[cfg(test)]
pub(crate) const REST_ROUTE_METHOD_COUNT: usize = 33;

pub(crate) fn build_router(app_state: ServerState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/auth/oidc/login", post(oidc_login_handler))
        .route("/api/auth/me", get(auth_me_handler))
        .route("/api/auth/logout", post(logout_handler))
        .route("/api/admin/health", get(admin_health_handler))
        .route("/api/admin/metrics", get(admin_metrics_handler))
        .route(
            "/api/admin/policy",
            get(get_enterprise_policy_handler).put(update_enterprise_policy_handler),
        )
        .route("/api/admin/audit", get(get_audit_events_handler))
        .route("/api/admin/reindex", post(admin_trigger_reindex_handler))
        .route(
            "/api/admin/indexing/pause",
            post(admin_pause_indexing_handler),
        )
        .route(
            "/api/admin/indexing/resume",
            post(admin_resume_indexing_handler),
        )
        .route("/api/stats", get(get_vault_stats_handler))
        .route("/api/indexing/status", get(get_indexing_status_handler))
        .route("/api/indexing/mode", post(set_indexing_mode_handler))
        .route("/api/indexing/trigger", post(trigger_reindex_handler))
        .route("/api/indexing/pause", post(pause_indexing_handler))
        .route("/api/indexing/resume", post(resume_indexing_handler))
        .route("/api/ask", post(ask_handler))
        .route("/api/ask_legacy", post(ask_legacy_handler))
        .route("/mcp", post(mcp::transport_http::mcp_http_handler))
        .route(
            "/api/settings",
            get(get_app_settings_handler).post(set_memory_settings_handler),
        )
        .route("/api/model-settings", get(get_model_settings_handler))
        .route("/api/model-settings", post(set_model_settings_handler))
        .route(
            "/api/model-settings/validate",
            get(validate_model_setup_handler),
        )
        .route(
            "/api/model-settings/list-models",
            post(list_provider_models_handler),
        )
        .route(
            "/api/model-settings/local-model-root",
            post(set_local_models_root_handler),
        )
        .route(
            "/api/model-settings/scan-local-model-files",
            post(scan_local_model_files_handler),
        )
        .route(
            "/api/model-settings/probe",
            post(probe_model_provider_handler),
        )
        .route("/api/model-settings/pull", post(pull_model_handler))
        .route("/api/settings/watch-root", post(set_watch_root_handler))
        .route("/api/settings/rank", post(rank_settings_query_handler))
        .route("/api/openapi.json", get(openapi_spec_handler))
        // 限流在路由内层（先于 handler，但在 CORS/request-id 之后），需 state 取限流器。
        .layer(axum::middleware::from_fn_with_state(
            app_state.clone(),
            rate_limit_middleware,
        ))
        .with_state(app_state)
        // request-id/trace 在最外层：先建立 span，再经 CORS、限流、handler，响应回写头部。
        .layer(axum::middleware::from_fn(request_id_trace_middleware))
        .layer(build_cors_layer())
}

fn build_cors_layer() -> CorsLayer {
    // 仅放行 API 实际用到的方法与头部（预检 OPTIONS 由 CorsLayer 自动处理）。
    CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::OPTIONS])
        .allow_headers([AUTHORIZATION, CONTENT_TYPE])
        .allow_origin(resolve_allowed_origins())
}

fn resolve_allowed_origins() -> Vec<HeaderValue> {
    let configured = std::env::var("MEMORI_ALLOWED_ORIGINS")
        .ok()
        .map(|value| {
            value
                .split(',')
                .filter_map(|item| {
                    let trimmed = item.trim();
                    (!trimmed.is_empty()).then_some(trimmed)
                })
                .filter_map(|item| item.parse::<HeaderValue>().ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !configured.is_empty() {
        return configured;
    }

    [
        "http://localhost:1420",
        "http://127.0.0.1:1420",
        "http://localhost:3000",
        "http://127.0.0.1:3000",
        "tauri://localhost",
        "http://tauri.localhost",
        "https://tauri.localhost",
    ]
    .into_iter()
    .filter_map(|origin| origin.parse::<HeaderValue>().ok())
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_allowed_origins_defaults_to_local_only() {
        unsafe {
            std::env::remove_var("MEMORI_ALLOWED_ORIGINS");
        }
        let origins = resolve_allowed_origins();
        let texts = origins
            .into_iter()
            .filter_map(|origin| origin.to_str().ok().map(str::to_string))
            .collect::<Vec<_>>();
        assert!(texts.iter().all(|origin| {
            origin.contains("localhost")
                || origin.contains("127.0.0.1")
                || origin == "tauri://localhost"
        }));
    }
}
