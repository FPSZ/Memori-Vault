use crate::*;

mod admin;
mod ask;
mod auth;
mod health;
mod indexing;
mod models;
mod settings;

pub(crate) use admin::*;
pub(crate) use ask::*;
pub(crate) use auth::*;
pub(crate) use health::*;
pub(crate) use indexing::*;
pub(crate) use models::*;
pub(crate) use settings::*;

pub(crate) fn build_router(app_state: ServerState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/auth/oidc/login", post(oidc_login_handler))
        .route("/api/auth/me", get(auth_me_handler))
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
        .with_state(app_state)
        .layer(
            CorsLayer::new()
                .allow_methods(Any)
                .allow_headers(Any)
                .allow_origin(Any),
        )
}
