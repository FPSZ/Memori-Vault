use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use memori_core::{
    AskResponseStructured, AskStatus, DEFAULT_CHAT_MODEL, DEFAULT_GRAPH_MODEL,
    DEFAULT_MODEL_ENDPOINT_OLLAMA, DEFAULT_MODEL_PROVIDER, DEFAULT_OLLAMA_EMBED_MODEL, EgressMode,
    EngineError, EnterpriseModelPolicy, IndexingConfig, IndexingMode, IndexingStatus,
    MEMORI_CHAT_MODEL_ENV, MEMORI_EMBED_MODEL_ENV, MEMORI_GRAPH_MODEL_ENV,
    MEMORI_MODEL_API_KEY_ENV, MEMORI_MODEL_ENDPOINT_ENV, MEMORI_MODEL_PROVIDER_ENV, MemoriEngine,
    ModelProvider, ResourceBudget, RuntimeModelConfig, ScheduleWindow, VaultStats,
    normalize_policy_endpoint, resolve_runtime_model_config_from_env, validate_provider_request,
    validate_runtime_model_settings,
};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio::time::{Duration, timeout};
use tower_http::cors::{Any, CorsLayer};
use tracing::{error, info, warn};
use uuid::Uuid;

const DEFAULT_RETRIEVE_TOP_K: usize = 20;
const ENGINE_SHUTDOWN_TIMEOUT_SECS: u64 = 8;
const PROVIDER_HTTP_TIMEOUT_SECS: u64 = 15;
const SETTINGS_APP_DIR_NAME: &str = "Memori-Vault";
const SETTINGS_FILE_NAME: &str = "settings.json";
const AUDIT_LOG_FILE_NAME: &str = "audit.log.jsonl";
const DEFAULT_SESSION_TTL_SECS: i64 = 8 * 60 * 60;

mod dto;
mod state;

pub(crate) use dto::*;
pub(crate) use state::*;

fn map_engine_api_error(err: EngineError) -> ApiError {
    match err {
        EngineError::IndexUnavailable { .. } => ApiError::conflict(
            "Index upgrade required. Search is temporarily unavailable until a full reindex completes.",
        ),
        EngineError::IndexRebuildInProgress { .. } => ApiError::service_unavailable(
            "Index upgrade in progress. Search is temporarily unavailable until reindex completes.",
        ),
        other => ApiError::internal(other.to_string()),
    }
}

#[tokio::main]
async fn main() {
    let _ = tracing_subscriber::fmt()
        .with_target(false)
        .with_thread_ids(true)
        .with_level(true)
        .try_init();

    let settings = match load_app_settings() {
        Ok(settings) => settings,
        Err(err) => {
            warn!(error = %err, "加载 settings.json 失败，回退默认配置");
            AppSettings::default()
        }
    };

    let watch_root = match resolve_watch_root_from_settings(&settings) {
        Ok(path) => path,
        Err(err) => {
            warn!(error = %err, "解析监听目录失败，回退当前工作目录");
            PathBuf::from(".")
        }
    };

    let engine = Arc::new(Mutex::new(None));
    let init_error = Arc::new(Mutex::new(None));
    if let Err(err) =
        replace_engine(&engine, &init_error, watch_root.clone(), "server_bootstrap").await
    {
        error!(error = %err, "memori-server bootstrap failed");
    }

    let app_state = ServerState {
        engine,
        init_error,
        sessions: Arc::new(Mutex::new(HashMap::new())),
        metrics: Arc::new(ServerMetrics::default()),
        audit_file_lock: Arc::new(Mutex::new(())),
    };
    let app = Router::new()
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
        .route("/api/settings", get(get_app_settings_handler))
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
        );

    let bind_addr = resolve_bind_addr();
    let listener = match tokio::net::TcpListener::bind(bind_addr).await {
        Ok(listener) => listener,
        Err(err) => {
            error!(addr = %bind_addr, error = %err, "启动 HTTP 监听失败");
            return;
        }
    };

    info!(addr = %bind_addr, "memori-server listening");
    if let Err(err) = axum::serve(listener, app).await {
        error!(error = %err, "memori-server exited with error");
    }
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { ok: true })
}

async fn oidc_login_handler(
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

async fn auth_me_handler(
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

async fn admin_health_handler(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let _ = require_session(&state, &headers, Role::Operator).await?;
    let init_error_message = state.init_error.lock().await.clone();
    let engine_guard = state.engine.lock().await;
    let ready = engine_guard.is_some() && init_error_message.is_none();
    drop(engine_guard);

    let indexing_status = if ready {
        get_indexing_status_handler(State(state.clone()))
            .await
            .ok()
            .map(|r| r.0)
    } else {
        None
    };
    Ok(Json(serde_json::json!({
        "ok": ready,
        "engine_ready": ready,
        "init_error": init_error_message,
        "indexing_status": indexing_status
    })))
}

async fn admin_metrics_handler(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<Json<ServerMetricsDto>, ApiError> {
    let _ = require_session(&state, &headers, Role::Operator).await?;
    Ok(Json(snapshot_metrics(&state.metrics)))
}

async fn get_enterprise_policy_handler(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<Json<EnterprisePolicyDto>, ApiError> {
    let _ = require_session(&state, &headers, Role::Operator).await?;
    let settings = load_app_settings().map_err(ApiError::internal)?;
    Ok(Json(resolve_enterprise_policy(&settings)))
}

async fn update_enterprise_policy_handler(
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

async fn get_audit_events_handler(
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

async fn admin_trigger_reindex_handler(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let session = require_session(&state, &headers, Role::Operator).await?;
    let response = trigger_reindex_handler(State(state.clone())).await?;
    append_audit_event(
        &state,
        AuditEventDto {
            actor: session.subject,
            action: "indexing.reindex".to_string(),
            resource: "indexing".to_string(),
            timestamp: unix_now_secs(),
            result: "ok".to_string(),
            metadata: serde_json::json!({}),
        },
    )
    .await;
    Ok(response)
}

async fn admin_pause_indexing_handler(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let session = require_session(&state, &headers, Role::Operator).await?;
    let response = pause_indexing_handler(State(state.clone())).await?;
    append_audit_event(
        &state,
        AuditEventDto {
            actor: session.subject,
            action: "indexing.pause".to_string(),
            resource: "indexing".to_string(),
            timestamp: unix_now_secs(),
            result: "ok".to_string(),
            metadata: serde_json::json!({}),
        },
    )
    .await;
    Ok(response)
}

async fn admin_resume_indexing_handler(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let session = require_session(&state, &headers, Role::Operator).await?;
    let response = resume_indexing_handler(State(state.clone())).await?;
    append_audit_event(
        &state,
        AuditEventDto {
            actor: session.subject,
            action: "indexing.resume".to_string(),
            resource: "indexing".to_string(),
            timestamp: unix_now_secs(),
            result: "ok".to_string(),
            metadata: serde_json::json!({}),
        },
    )
    .await;
    Ok(response)
}

async fn ask_handler(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(payload): Json<AskRequest>,
) -> Result<Json<AskResponseStructured>, ApiError> {
    state.metrics.total_requests.fetch_add(1, Ordering::Relaxed);
    state.metrics.ask_requests.fetch_add(1, Ordering::Relaxed);
    let ask_started_at = std::time::Instant::now();

    let query = payload.query.trim().to_string();
    if query.is_empty() {
        state
            .metrics
            .failed_requests
            .fetch_add(1, Ordering::Relaxed);
        state.metrics.ask_failed.fetch_add(1, Ordering::Relaxed);
        return Ok(Json(AskResponseStructured {
            status: AskStatus::InsufficientEvidence,
            answer: String::new(),
            question: query,
            scope_paths: Vec::new(),
            citations: Vec::new(),
            evidence: Vec::new(),
            metrics: Default::default(),
        }));
    }

    let actor = resolve_actor_subject(&state, &headers).await;
    let policy = resolve_enterprise_policy(&load_app_settings().map_err(ApiError::internal)?);
    if let Err(violation) = validate_runtime_model_settings(
        &to_model_policy(&policy),
        &resolve_runtime_model_config_from_env(),
    ) {
        append_policy_violation_audit(
            &state,
            actor.clone(),
            "ask",
            None,
            None,
            &[],
            &violation.message,
        )
        .await;
        return Err(ApiError::forbidden(violation.message));
    }

    let init_error_message = state.init_error.lock().await.clone();
    let engine_guard = state.engine.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        state
            .metrics
            .failed_requests
            .fetch_add(1, Ordering::Relaxed);
        state.metrics.ask_failed.fetch_add(1, Ordering::Relaxed);
        if let Some(message) = init_error_message {
            return Err(ApiError::internal(format!("引擎初始化失败: {message}")));
        }
        return Err(ApiError::internal("引擎尚在初始化中，请稍后重试。"));
    };

    let top_k = normalize_top_k(payload.top_k);
    let scope_paths = normalize_scope_paths(payload.scope_paths);
    let scope_refs = if scope_paths.is_empty() {
        None
    } else {
        Some(scope_paths.as_slice())
    };

    let response = engine
        .ask_structured(&query, payload.lang.as_deref(), scope_refs, payload.top_k)
        .await
        .map_err(|err| {
            state
                .metrics
                .failed_requests
                .fetch_add(1, Ordering::Relaxed);
            state.metrics.ask_failed.fetch_add(1, Ordering::Relaxed);
            map_engine_api_error(err)
        })?;

    let elapsed_ms = ask_started_at.elapsed().as_millis() as u64;
    state
        .metrics
        .ask_latency_total_ms
        .fetch_add(elapsed_ms, Ordering::Relaxed);
    append_audit_event(
        &state,
        AuditEventDto {
            actor,
            action: "query.ask".to_string(),
            resource: "vault".to_string(),
            timestamp: unix_now_secs(),
            result: "ok".to_string(),
            metadata: serde_json::json!({
                "top_k": top_k,
                "scope_count": scope_paths.len(),
                "status": response.status,
                "doc_candidate_count": response.metrics.doc_candidate_count,
                "final_evidence_count": response.metrics.final_evidence_count,
                "elapsed_ms": elapsed_ms
            }),
        },
    )
    .await;

    Ok(Json(response))
}

#[derive(Debug, Clone, Serialize)]
struct AskLegacyResponse {
    answer: String,
}

async fn ask_legacy_handler(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(payload): Json<AskRequest>,
) -> Result<Json<AskLegacyResponse>, ApiError> {
    let response = ask_handler(State(state), headers, Json(payload)).await?;
    Ok(Json(AskLegacyResponse {
        answer: format_legacy_answer(&response.0),
    }))
}

async fn get_vault_stats_handler(
    State(state): State<ServerState>,
) -> Result<Json<VaultStats>, ApiError> {
    let init_error_message = state.init_error.lock().await.clone();
    let engine_guard = state.engine.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_message {
            return Err(ApiError::internal(format!("引擎初始化失败: {message}")));
        }
        return Err(ApiError::internal("引擎尚在初始化中，请稍后重试。"));
    };
    let stats = engine
        .get_vault_stats()
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    Ok(Json(stats))
}

async fn get_indexing_status_handler(
    State(state): State<ServerState>,
) -> Result<Json<IndexingStatus>, ApiError> {
    let init_error_message = state.init_error.lock().await.clone();
    let engine_guard = state.engine.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_message {
            return Err(ApiError::internal(format!("引擎初始化失败: {message}")));
        }
        return Err(ApiError::internal("引擎尚在初始化中，请稍后重试。"));
    };
    let status = engine
        .get_indexing_status()
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    Ok(Json(status))
}

async fn set_indexing_mode_handler(
    State(state): State<ServerState>,
    Json(payload): Json<SetIndexingModePayload>,
) -> Result<Json<AppSettingsDto>, ApiError> {
    let mut settings = load_app_settings().map_err(ApiError::internal)?;
    let mode = IndexingMode::from_value(&payload.indexing_mode);
    let budget = ResourceBudget::from_value(&payload.resource_budget);
    let schedule_window = if mode == IndexingMode::Scheduled {
        Some(ScheduleWindow {
            start: payload
                .schedule_start
                .unwrap_or_else(|| "00:00".to_string()),
            end: payload.schedule_end.unwrap_or_else(|| "06:00".to_string()),
        })
    } else {
        None
    };
    settings.indexing_mode = Some(mode.as_str().to_string());
    settings.resource_budget = Some(budget.as_str().to_string());
    settings.schedule_start = schedule_window.as_ref().map(|w| w.start.clone());
    settings.schedule_end = schedule_window.as_ref().map(|w| w.end.clone());
    save_app_settings(&settings).map_err(ApiError::internal)?;

    let init_error_message = state.init_error.lock().await.clone();
    let engine_guard = state.engine.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_message {
            return Err(ApiError::internal(format!("引擎初始化失败: {message}")));
        }
        return Err(ApiError::internal("引擎尚在初始化中，请稍后重试。"));
    };
    engine
        .set_indexing_config(IndexingConfig {
            mode,
            resource_budget: budget,
            schedule_window,
        })
        .await;

    let watch_root = resolve_watch_root_from_settings(&settings).map_err(ApiError::internal)?;
    let indexing = resolve_indexing_config(&settings);
    Ok(Json(AppSettingsDto {
        watch_root: watch_root.to_string_lossy().to_string(),
        language: settings.language,
        indexing_mode: indexing.mode.as_str().to_string(),
        resource_budget: indexing.resource_budget.as_str().to_string(),
        schedule_start: indexing.schedule_window.as_ref().map(|w| w.start.clone()),
        schedule_end: indexing.schedule_window.as_ref().map(|w| w.end.clone()),
    }))
}

async fn trigger_reindex_handler(
    State(state): State<ServerState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let init_error_message = state.init_error.lock().await.clone();
    let engine_guard = state.engine.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_message {
            return Err(ApiError::internal(format!("引擎初始化失败: {message}")));
        }
        return Err(ApiError::internal("引擎尚在初始化中，请稍后重试。"));
    };
    engine
        .trigger_reindex()
        .await
        .map_err(map_engine_api_error)?;
    Ok(Json(serde_json::json!({
        "task_id": format!("reindex-{}", chrono_like_now_token())
    })))
}

async fn pause_indexing_handler(
    State(state): State<ServerState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let init_error_message = state.init_error.lock().await.clone();
    let engine_guard = state.engine.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_message {
            return Err(ApiError::internal(format!("引擎初始化失败: {message}")));
        }
        return Err(ApiError::internal("引擎尚在初始化中，请稍后重试。"));
    };
    engine.pause_indexing().await;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn resume_indexing_handler(
    State(state): State<ServerState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let init_error_message = state.init_error.lock().await.clone();
    let engine_guard = state.engine.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_message {
            return Err(ApiError::internal(format!("引擎初始化失败: {message}")));
        }
        return Err(ApiError::internal("引擎尚在初始化中，请稍后重试。"));
    };
    engine.resume_indexing().await;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn get_app_settings_handler() -> Result<Json<AppSettingsDto>, ApiError> {
    let settings = load_app_settings().map_err(ApiError::internal)?;
    let watch_root = resolve_watch_root_from_settings(&settings).map_err(ApiError::internal)?;
    let indexing = resolve_indexing_config(&settings);
    Ok(Json(AppSettingsDto {
        watch_root: watch_root.to_string_lossy().to_string(),
        language: settings.language,
        indexing_mode: indexing.mode.as_str().to_string(),
        resource_budget: indexing.resource_budget.as_str().to_string(),
        schedule_start: indexing.schedule_window.as_ref().map(|w| w.start.clone()),
        schedule_end: indexing.schedule_window.as_ref().map(|w| w.end.clone()),
    }))
}

async fn get_model_settings_handler() -> Result<Json<ModelSettingsDto>, ApiError> {
    let settings = load_app_settings().map_err(ApiError::internal)?;
    Ok(Json(resolve_model_settings(&settings)))
}

async fn set_model_settings_handler(
    State(state): State<ServerState>,
    Json(payload): Json<ModelSettingsDto>,
) -> Result<Json<ModelSettingsDto>, ApiError> {
    let mut settings = load_app_settings().map_err(ApiError::internal)?;
    let normalized = normalize_model_settings_payload(payload).map_err(ApiError::bad_request)?;
    let policy = resolve_enterprise_policy(&settings);
    let active_runtime = resolve_active_runtime_settings(&normalized);
    if let Err(violation) = validate_runtime_model_settings(
        &to_model_policy(&policy),
        &to_runtime_model_config(&active_runtime),
    ) {
        append_policy_violation_audit(
            &state,
            "anonymous".to_string(),
            "set_model_settings",
            Some(active_runtime.provider),
            Some(&active_runtime.endpoint),
            &[
                active_runtime.chat_model.clone(),
                active_runtime.graph_model.clone(),
                active_runtime.embed_model.clone(),
            ],
            &violation.message,
        )
        .await;
        return Err(ApiError::forbidden(violation.message));
    }
    settings.active_provider = Some(normalized.active_provider.clone());
    settings.local_endpoint = Some(normalized.local_profile.endpoint.clone());
    settings.local_models_root = normalized.local_profile.models_root.clone();
    settings.local_chat_model = Some(normalized.local_profile.chat_model.clone());
    settings.local_graph_model = Some(normalized.local_profile.graph_model.clone());
    settings.local_embed_model = Some(normalized.local_profile.embed_model.clone());
    settings.remote_endpoint = Some(normalized.remote_profile.endpoint.clone());
    settings.remote_api_key = normalized.remote_profile.api_key.clone();
    settings.remote_chat_model = Some(normalized.remote_profile.chat_model.clone());
    settings.remote_graph_model = Some(normalized.remote_profile.graph_model.clone());
    settings.remote_embed_model = Some(normalized.remote_profile.embed_model.clone());
    save_app_settings(&settings).map_err(ApiError::internal)?;
    apply_model_settings_to_env(resolve_active_runtime_settings(&normalized));

    let watch_root = resolve_watch_root_from_settings(&settings).map_err(ApiError::internal)?;
    replace_engine(
        &state.engine,
        &state.init_error,
        watch_root,
        "settings_model_update",
    )
    .await
    .map_err(ApiError::internal)?;

    Ok(Json(normalized))
}

async fn validate_model_setup_handler(
    State(state): State<ServerState>,
) -> Result<Json<ModelAvailabilityDto>, ApiError> {
    let settings = load_app_settings().map_err(ApiError::internal)?;
    let model_settings = resolve_model_settings(&settings);
    let active = resolve_active_runtime_settings(&model_settings);
    let policy = resolve_enterprise_policy(&settings);
    if let Err(violation) = validate_runtime_model_settings(
        &to_model_policy(&policy),
        &to_runtime_model_config(&active),
    ) {
        append_policy_violation_audit(
            &state,
            "anonymous".to_string(),
            "validate_model_setup",
            Some(active.provider),
            Some(&active.endpoint),
            &[
                active.chat_model.clone(),
                active.graph_model.clone(),
                active.embed_model.clone(),
            ],
            &violation.message,
        )
        .await;
        return Err(ApiError::forbidden(violation.message));
    }
    let provider = active.provider;
    let models = fetch_provider_models(
        provider,
        &active.endpoint,
        active.api_key.as_deref(),
        active.models_root.as_deref(),
    )
    .await;

    match models {
        Ok(models) => {
            let merged = models.merged;
            let mut missing_roles = Vec::new();
            if !model_exists(&merged, &active.chat_model) {
                missing_roles.push("chat".to_string());
            }
            if !model_exists(&merged, &active.graph_model) {
                missing_roles.push("graph".to_string());
            }
            if !model_exists(&merged, &active.embed_model) {
                missing_roles.push("embed".to_string());
            }
            Ok(Json(ModelAvailabilityDto {
                reachable: true,
                models: merged,
                missing_roles,
                errors: Vec::new(),
                checked_provider: Some(provider_to_string(provider)),
            }))
        }
        Err(err) => Ok(Json(ModelAvailabilityDto {
            reachable: false,
            models: Vec::new(),
            missing_roles: vec!["chat".to_string(), "graph".to_string(), "embed".to_string()],
            errors: vec![ModelErrorItem {
                code: err.code,
                message: err.message,
            }],
            checked_provider: Some(provider_to_string(provider)),
        })),
    }
}

async fn list_provider_models_handler(
    State(state): State<ServerState>,
    Json(payload): Json<ListProviderModelsRequest>,
) -> Result<Json<ProviderModelsDto>, ApiError> {
    let settings = load_app_settings().map_err(ApiError::internal)?;
    let policy = resolve_enterprise_policy(&settings);
    let provider = ModelProvider::from_value(&payload.provider);
    let endpoint = normalize_endpoint(provider, &payload.endpoint);
    let api_key = normalize_optional_text(payload.api_key);
    let models_root = normalize_optional_text(payload.models_root);
    if let Err(violation) =
        validate_provider_request(&to_model_policy(&policy), provider, &endpoint, &[])
    {
        append_policy_violation_audit(
            &state,
            "anonymous".to_string(),
            "list_provider_models",
            Some(provider),
            Some(&endpoint),
            &[],
            &violation.message,
        )
        .await;
        return Err(ApiError::forbidden(violation.message));
    }
    if provider == ModelProvider::OllamaLocal {
        let from_folder = models_root
            .as_deref()
            .map(PathBuf::from)
            .map(|root| scan_local_model_files_from_root(&root))
            .transpose()
            .map_err(ApiError::bad_request)?
            .unwrap_or_default();
        let from_service = list_ollama_models(&endpoint).await.unwrap_or_default();
        return Ok(Json(merge_model_candidates(from_folder, from_service)));
    }
    let models = fetch_provider_models(
        provider,
        &endpoint,
        api_key.as_deref(),
        models_root.as_deref(),
    )
    .await
    .map_err(|err| ApiError::internal(format!("{}: {}", err.code, err.message)))?;
    Ok(Json(models))
}

async fn probe_model_provider_handler(
    State(state): State<ServerState>,
    Json(payload): Json<ProbeProviderRequest>,
) -> Result<Json<ModelAvailabilityDto>, ApiError> {
    let settings = load_app_settings().map_err(ApiError::internal)?;
    let policy = resolve_enterprise_policy(&settings);
    let provider = ModelProvider::from_value(&payload.provider);
    let endpoint = normalize_endpoint(provider, &payload.endpoint);
    let api_key = normalize_optional_text(payload.api_key);
    let models_root = normalize_optional_text(payload.models_root);
    if let Err(violation) =
        validate_provider_request(&to_model_policy(&policy), provider, &endpoint, &[])
    {
        append_policy_violation_audit(
            &state,
            "anonymous".to_string(),
            "probe_model_provider",
            Some(provider),
            Some(&endpoint),
            &[],
            &violation.message,
        )
        .await;
        return Err(ApiError::forbidden(violation.message));
    }
    let result = fetch_provider_models(
        provider,
        &endpoint,
        api_key.as_deref(),
        models_root.as_deref(),
    )
    .await;
    match result {
        Ok(models) => Ok(Json(ModelAvailabilityDto {
            reachable: true,
            models: models.merged,
            missing_roles: Vec::new(),
            errors: Vec::new(),
            checked_provider: Some(provider_to_string(provider)),
        })),
        Err(err) => Ok(Json(ModelAvailabilityDto {
            reachable: false,
            models: Vec::new(),
            missing_roles: Vec::new(),
            errors: vec![ModelErrorItem {
                code: err.code,
                message: err.message,
            }],
            checked_provider: Some(provider_to_string(provider)),
        })),
    }
}

async fn pull_model_handler(
    State(state): State<ServerState>,
    Json(payload): Json<PullModelRequest>,
) -> Result<Json<ModelAvailabilityDto>, ApiError> {
    let model = payload.model.trim().to_string();
    if model.is_empty() {
        return Err(ApiError::bad_request("模型名不能为空"));
    }
    let provider = ModelProvider::from_value(&payload.provider);
    if provider != ModelProvider::OllamaLocal {
        return Err(ApiError::bad_request("仅本地 Ollama 模式支持拉取模型"));
    }
    let endpoint = normalize_endpoint(provider, &payload.endpoint);
    let policy = resolve_enterprise_policy(&load_app_settings().map_err(ApiError::internal)?);
    if let Err(violation) =
        validate_provider_request(&to_model_policy(&policy), provider, &endpoint, &[])
    {
        append_policy_violation_audit(
            &state,
            "anonymous".to_string(),
            "pull_model",
            Some(provider),
            Some(&endpoint),
            &[],
            &violation.message,
        )
        .await;
        return Err(ApiError::forbidden(violation.message));
    }
    let api_key = normalize_optional_text(payload.api_key);
    pull_ollama_model(&endpoint, &model, api_key.as_deref())
        .await
        .map_err(ApiError::internal)?;
    validate_model_setup_handler(State(state)).await
}

async fn set_local_models_root_handler(
    Json(payload): Json<SetLocalModelsRootRequest>,
) -> Result<Json<ModelSettingsDto>, ApiError> {
    let mut settings = load_app_settings().map_err(ApiError::internal)?;
    let path = normalize_optional_text(Some(payload.path));
    if let Some(root_path) = path.as_deref() {
        let root = PathBuf::from(root_path);
        if !root.exists() {
            return Err(ApiError::bad_request(format!(
                "模型目录不存在: {}",
                root.display()
            )));
        }
        if !root.is_dir() {
            return Err(ApiError::bad_request(format!(
                "路径不是目录: {}",
                root.display()
            )));
        }
        settings.local_models_root = Some(
            root.canonicalize()
                .unwrap_or(root)
                .to_string_lossy()
                .to_string(),
        );
    } else {
        settings.local_models_root = None;
    }
    save_app_settings(&settings).map_err(ApiError::internal)?;
    Ok(Json(resolve_model_settings(&settings)))
}

async fn scan_local_model_files_handler(
    Json(payload): Json<ScanLocalModelFilesRequest>,
) -> Result<Json<Vec<String>>, ApiError> {
    let root = normalize_optional_text(payload.root);
    if let Some(root) = root {
        let models = scan_local_model_files_from_root(&PathBuf::from(root))
            .map_err(ApiError::bad_request)?;
        return Ok(Json(models));
    }
    let settings = load_app_settings().map_err(ApiError::internal)?;
    let model_settings = resolve_model_settings(&settings);
    if let Some(root) = model_settings.local_profile.models_root {
        let models = scan_local_model_files_from_root(&PathBuf::from(root))
            .map_err(ApiError::bad_request)?;
        return Ok(Json(models));
    }
    Ok(Json(Vec::new()))
}

async fn set_watch_root_handler(
    State(state): State<ServerState>,
    Json(payload): Json<SetWatchRootRequest>,
) -> Result<Json<AppSettingsDto>, ApiError> {
    let trimmed = payload.path.trim();
    if trimmed.is_empty() {
        return Err(ApiError::bad_request("目录路径为空，无法保存。"));
    }

    let watch_root = PathBuf::from(trimmed);
    if !watch_root.exists() {
        return Err(ApiError::bad_request(format!(
            "目录不存在: {}",
            watch_root.display()
        )));
    }
    if !watch_root.is_dir() {
        return Err(ApiError::bad_request(format!(
            "路径不是目录: {}",
            watch_root.display()
        )));
    }

    let canonical = watch_root
        .canonicalize()
        .map_err(|err| ApiError::bad_request(format!("规范化目录失败: {err}")))?;

    let mut settings = load_app_settings().map_err(ApiError::internal)?;
    settings.watch_root = Some(canonical.to_string_lossy().to_string());
    save_app_settings(&settings).map_err(ApiError::internal)?;

    replace_engine(
        &state.engine,
        &state.init_error,
        canonical.clone(),
        "settings_watch_root_update",
    )
    .await
    .map_err(ApiError::internal)?;

    let indexing = resolve_indexing_config(&settings);
    Ok(Json(AppSettingsDto {
        watch_root: canonical.to_string_lossy().to_string(),
        language: settings.language,
        indexing_mode: indexing.mode.as_str().to_string(),
        resource_budget: indexing.resource_budget.as_str().to_string(),
        schedule_start: indexing.schedule_window.as_ref().map(|w| w.start.clone()),
        schedule_end: indexing.schedule_window.as_ref().map(|w| w.end.clone()),
    }))
}

async fn rank_settings_query_handler(
    State(state): State<ServerState>,
    Json(payload): Json<RankSettingsRequest>,
) -> Result<Json<RankSettingsResponse>, ApiError> {
    let query = payload.query.trim();
    if query.is_empty() || payload.candidates.is_empty() {
        return Ok(Json(RankSettingsResponse { keys: Vec::new() }));
    }

    let init_error_message = state.init_error.lock().await.clone();
    let engine_guard = state.engine.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_message {
            return Err(ApiError::internal(format!("引擎初始化失败: {message}")));
        }
        return Err(ApiError::internal("引擎尚在初始化中，请稍后重试。"));
    };

    let mut candidate_lines = Vec::with_capacity(payload.candidates.len());
    for item in &payload.candidates {
        candidate_lines.push(format!("{} => {}", item.key.trim(), item.text.trim()));
    }

    let prompt = match normalize_language(payload.lang.as_deref()) {
        Some("zh-CN") => format!(
            "你是设置检索助手。用户搜索词：{query}\n候选设置项：\n{}\n\n请仅返回 JSON 数组，内容为最匹配的 key，最多 3 个。示例：[\"basic\",\"models\"]。\n禁止输出解释文字。",
            candidate_lines.join("\n")
        ),
        _ => format!(
            "You are a settings retrieval assistant.\nQuery: {query}\nCandidates:\n{}\n\nReturn only a JSON array of best-matching keys, max 3. Example: [\"basic\",\"models\"].\nDo not output explanations.",
            candidate_lines.join("\n")
        ),
    };

    let answer = engine
        .generate_answer(&prompt, "", "")
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

    let candidate_keys: HashSet<String> = payload
        .candidates
        .iter()
        .map(|c| c.key.trim().to_string())
        .collect();

    if let Ok(parsed) = serde_json::from_str::<Vec<String>>(&answer) {
        let matched = parsed
            .into_iter()
            .filter(|key| candidate_keys.contains(key.trim()))
            .collect::<Vec<_>>();
        if !matched.is_empty() {
            return Ok(Json(RankSettingsResponse { keys: matched }));
        }
    }

    if let (Some(start), Some(end)) = (answer.find('['), answer.rfind(']'))
        && start < end
    {
        let json_slice = &answer[start..=end];
        if let Ok(parsed) = serde_json::from_str::<Vec<String>>(json_slice) {
            let matched = parsed
                .into_iter()
                .filter(|key| candidate_keys.contains(key.trim()))
                .collect::<Vec<_>>();
            if !matched.is_empty() {
                return Ok(Json(RankSettingsResponse { keys: matched }));
            }
        }
    }

    let lower_answer = answer.to_ascii_lowercase();
    let fallback = payload
        .candidates
        .iter()
        .filter_map(|candidate| {
            let key = candidate.key.trim().to_string();
            if key.is_empty() {
                return None;
            }
            if lower_answer.contains(&key.to_ascii_lowercase()) {
                Some(key)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    Ok(Json(RankSettingsResponse { keys: fallback }))
}

async fn replace_engine(
    engine_slot: &Arc<Mutex<Option<MemoriEngine>>>,
    init_error: &Arc<Mutex<Option<String>>>,
    watch_root: PathBuf,
    reason: &str,
) -> Result<(), String> {
    let previous_engine = {
        let mut guard = engine_slot.lock().await;
        guard.take()
    };

    if let Some(engine) = previous_engine {
        match timeout(
            Duration::from_secs(ENGINE_SHUTDOWN_TIMEOUT_SECS),
            engine.shutdown(),
        )
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                warn!(error = %err, "关闭旧引擎失败，继续尝试重建");
            }
            Err(_) => {
                warn!(
                    timeout_secs = ENGINE_SHUTDOWN_TIMEOUT_SECS,
                    "关闭旧引擎超时，继续尝试重建"
                );
            }
        }
    }

    let result: Result<(), String> = async {
        let settings = load_app_settings()?;
        let policy = resolve_enterprise_policy(&settings);
        let model_settings = resolve_model_settings(&settings);
        let active_runtime = resolve_active_runtime_settings(&model_settings);
        validate_runtime_model_settings(
            &to_model_policy(&policy),
            &to_runtime_model_config(&active_runtime),
        )
        .map_err(|violation| violation.message)?;
        apply_model_settings_to_env(active_runtime);

        let mut new_engine =
            MemoriEngine::bootstrap(watch_root.clone()).map_err(|err| err.to_string())?;
        new_engine
            .set_indexing_config(resolve_indexing_config(&settings))
            .await;
        new_engine.start_daemon().map_err(|err| err.to_string())?;

        {
            let mut guard = engine_slot.lock().await;
            *guard = Some(new_engine);
        }
        {
            let mut init_guard = init_error.lock().await;
            *init_guard = None;
        }

        info!(
            reason = reason,
            watch_root = %watch_root.display(),
            "memori-server daemon started"
        );

        Ok(())
    }
    .await;

    if let Err(err) = &result {
        let mut init_guard = init_error.lock().await;
        *init_guard = Some(err.clone());
    }

    result
}

fn resolve_bind_addr() -> SocketAddr {
    let default_addr = SocketAddr::from(([127, 0, 0, 1], 3757));
    let env_addr = std::env::var("MEMORI_SERVER_ADDR").ok();
    let Some(addr) = env_addr else {
        return default_addr;
    };

    match addr.parse::<SocketAddr>() {
        Ok(parsed) => parsed,
        Err(err) => {
            warn!(value = %addr, error = %err, "MEMORI_SERVER_ADDR 非法，回退默认地址");
            default_addr
        }
    }
}

fn app_settings_file_path() -> Result<PathBuf, String> {
    let config_root = dirs::config_dir().ok_or_else(|| "无法获取用户配置目录".to_string())?;
    Ok(config_root
        .join(SETTINGS_APP_DIR_NAME)
        .join(SETTINGS_FILE_NAME))
}

fn load_app_settings() -> Result<AppSettings, String> {
    let settings_file = app_settings_file_path()?;
    if !settings_file.exists() {
        return Ok(AppSettings::default());
    }

    let content = fs::read_to_string(&settings_file)
        .map_err(|err| format!("读取配置失败({}): {err}", settings_file.display()))?;
    serde_json::from_str(&content)
        .map_err(|err| format!("解析配置失败({}): {err}", settings_file.display()))
}

fn save_app_settings(settings: &AppSettings) -> Result<(), String> {
    let settings_file = app_settings_file_path()?;
    if let Some(parent) = settings_file.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("创建配置目录失败({}): {err}", parent.display()))?;
    }

    let content =
        serde_json::to_string_pretty(settings).map_err(|err| format!("序列化配置失败: {err}"))?;
    fs::write(&settings_file, content)
        .map_err(|err| format!("写入配置失败({}): {err}", settings_file.display()))
}

fn resolve_watch_root_from_settings(settings: &AppSettings) -> Result<PathBuf, String> {
    if let Some(path) = settings.watch_root.as_deref() {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }

    if let Ok(path) = std::env::var("MEMORI_WATCH_ROOT") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }

    std::env::current_dir().map_err(|err| format!("获取当前工作目录失败: {err}"))
}

#[derive(Debug, Clone)]
struct ActiveRuntimeModelSettings {
    provider: ModelProvider,
    endpoint: String,
    api_key: Option<String>,
    models_root: Option<String>,
    chat_model: String,
    graph_model: String,
    embed_model: String,
}

fn to_runtime_model_config(settings: &ActiveRuntimeModelSettings) -> RuntimeModelConfig {
    RuntimeModelConfig {
        provider: settings.provider,
        endpoint: settings.endpoint.clone(),
        api_key: settings.api_key.clone(),
        chat_model: settings.chat_model.clone(),
        graph_model: settings.graph_model.clone(),
        embed_model: settings.embed_model.clone(),
    }
}

fn to_model_policy(policy: &EnterprisePolicyDto) -> EnterpriseModelPolicy {
    EnterpriseModelPolicy {
        egress_mode: policy.egress_mode,
        allowed_model_endpoints: policy.allowed_model_endpoints.clone(),
        allowed_models: policy.allowed_models.clone(),
    }
}

fn provider_to_string(provider: ModelProvider) -> String {
    if provider == ModelProvider::OpenAiCompatible {
        "openai_compatible".to_string()
    } else {
        "ollama_local".to_string()
    }
}

fn resolve_model_settings(settings: &AppSettings) -> ModelSettingsDto {
    let fallback_provider = settings.active_provider.clone().unwrap_or_else(|| {
        settings.provider.clone().unwrap_or_else(|| {
            std::env::var(MEMORI_MODEL_PROVIDER_ENV)
                .unwrap_or_else(|_| DEFAULT_MODEL_PROVIDER.to_string())
        })
    });
    let active_provider = ModelProvider::from_value(&fallback_provider);
    let env_provider = std::env::var(MEMORI_MODEL_PROVIDER_ENV)
        .ok()
        .map(|value| ModelProvider::from_value(&value))
        .unwrap_or(active_provider);

    let local_endpoint = settings
        .local_endpoint
        .clone()
        .or_else(|| {
            if ModelProvider::from_value(
                settings
                    .provider
                    .as_deref()
                    .unwrap_or(fallback_provider.as_str()),
            ) == ModelProvider::OllamaLocal
            {
                settings.endpoint.clone()
            } else {
                None
            }
        })
        .or_else(|| {
            if env_provider == ModelProvider::OllamaLocal {
                std::env::var(MEMORI_MODEL_ENDPOINT_ENV).ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| DEFAULT_MODEL_ENDPOINT_OLLAMA.to_string());

    let remote_endpoint = settings
        .remote_endpoint
        .clone()
        .or_else(|| {
            if ModelProvider::from_value(
                settings
                    .provider
                    .as_deref()
                    .unwrap_or(fallback_provider.as_str()),
            ) == ModelProvider::OpenAiCompatible
            {
                settings.endpoint.clone()
            } else {
                None
            }
        })
        .or_else(|| {
            if env_provider == ModelProvider::OpenAiCompatible {
                std::env::var(MEMORI_MODEL_ENDPOINT_ENV).ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| memori_core::DEFAULT_MODEL_ENDPOINT_OPENAI.to_string());

    let local_chat_model = settings
        .local_chat_model
        .clone()
        .or_else(|| {
            if ModelProvider::from_value(
                settings
                    .provider
                    .as_deref()
                    .unwrap_or(fallback_provider.as_str()),
            ) == ModelProvider::OllamaLocal
            {
                settings.chat_model.clone()
            } else {
                None
            }
        })
        .or_else(|| {
            if env_provider == ModelProvider::OllamaLocal {
                std::env::var(MEMORI_CHAT_MODEL_ENV).ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| DEFAULT_CHAT_MODEL.to_string());

    let local_graph_model = settings
        .local_graph_model
        .clone()
        .or_else(|| {
            if ModelProvider::from_value(
                settings
                    .provider
                    .as_deref()
                    .unwrap_or(fallback_provider.as_str()),
            ) == ModelProvider::OllamaLocal
            {
                settings.graph_model.clone()
            } else {
                None
            }
        })
        .or_else(|| {
            if env_provider == ModelProvider::OllamaLocal {
                std::env::var(MEMORI_GRAPH_MODEL_ENV).ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| DEFAULT_GRAPH_MODEL.to_string());

    let local_embed_model = settings
        .local_embed_model
        .clone()
        .or_else(|| {
            if ModelProvider::from_value(
                settings
                    .provider
                    .as_deref()
                    .unwrap_or(fallback_provider.as_str()),
            ) == ModelProvider::OllamaLocal
            {
                settings.embed_model.clone()
            } else {
                None
            }
        })
        .or_else(|| {
            if env_provider == ModelProvider::OllamaLocal {
                std::env::var(MEMORI_EMBED_MODEL_ENV).ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| DEFAULT_OLLAMA_EMBED_MODEL.to_string());

    let remote_chat_model = settings
        .remote_chat_model
        .clone()
        .or_else(|| {
            if ModelProvider::from_value(
                settings
                    .provider
                    .as_deref()
                    .unwrap_or(fallback_provider.as_str()),
            ) == ModelProvider::OpenAiCompatible
            {
                settings.chat_model.clone()
            } else {
                None
            }
        })
        .or_else(|| {
            if env_provider == ModelProvider::OpenAiCompatible {
                std::env::var(MEMORI_CHAT_MODEL_ENV).ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| DEFAULT_CHAT_MODEL.to_string());

    let remote_graph_model = settings
        .remote_graph_model
        .clone()
        .or_else(|| {
            if ModelProvider::from_value(
                settings
                    .provider
                    .as_deref()
                    .unwrap_or(fallback_provider.as_str()),
            ) == ModelProvider::OpenAiCompatible
            {
                settings.graph_model.clone()
            } else {
                None
            }
        })
        .or_else(|| {
            if env_provider == ModelProvider::OpenAiCompatible {
                std::env::var(MEMORI_GRAPH_MODEL_ENV).ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| DEFAULT_GRAPH_MODEL.to_string());

    let remote_embed_model = settings
        .remote_embed_model
        .clone()
        .or_else(|| {
            if ModelProvider::from_value(
                settings
                    .provider
                    .as_deref()
                    .unwrap_or(fallback_provider.as_str()),
            ) == ModelProvider::OpenAiCompatible
            {
                settings.embed_model.clone()
            } else {
                None
            }
        })
        .or_else(|| {
            if env_provider == ModelProvider::OpenAiCompatible {
                std::env::var(MEMORI_EMBED_MODEL_ENV).ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| DEFAULT_OLLAMA_EMBED_MODEL.to_string());

    let remote_api_key = settings
        .remote_api_key
        .clone()
        .or_else(|| {
            if ModelProvider::from_value(
                settings
                    .provider
                    .as_deref()
                    .unwrap_or(fallback_provider.as_str()),
            ) == ModelProvider::OpenAiCompatible
            {
                settings.api_key.clone()
            } else {
                None
            }
        })
        .or_else(|| {
            if env_provider == ModelProvider::OpenAiCompatible {
                std::env::var(MEMORI_MODEL_API_KEY_ENV).ok()
            } else {
                None
            }
        })
        .and_then(|value| normalize_optional_text(Some(value)));

    ModelSettingsDto {
        active_provider: provider_to_string(active_provider),
        local_profile: LocalModelProfileDto {
            endpoint: normalize_endpoint(ModelProvider::OllamaLocal, &local_endpoint),
            models_root: normalize_optional_text(settings.local_models_root.clone()),
            chat_model: local_chat_model,
            graph_model: local_graph_model,
            embed_model: local_embed_model,
        },
        remote_profile: RemoteModelProfileDto {
            endpoint: normalize_endpoint(ModelProvider::OpenAiCompatible, &remote_endpoint),
            api_key: remote_api_key,
            chat_model: remote_chat_model,
            graph_model: remote_graph_model,
            embed_model: remote_embed_model,
        },
    }
}

fn normalize_model_settings_payload(payload: ModelSettingsDto) -> Result<ModelSettingsDto, String> {
    let active_provider = ModelProvider::from_value(&payload.active_provider);
    let local_endpoint =
        normalize_endpoint(ModelProvider::OllamaLocal, &payload.local_profile.endpoint);
    let remote_endpoint = normalize_endpoint(
        ModelProvider::OpenAiCompatible,
        &payload.remote_profile.endpoint,
    );

    let local_chat_model = payload.local_profile.chat_model.trim().to_string();
    let local_graph_model = payload.local_profile.graph_model.trim().to_string();
    let local_embed_model = payload.local_profile.embed_model.trim().to_string();
    let remote_chat_model = payload.remote_profile.chat_model.trim().to_string();
    let remote_graph_model = payload.remote_profile.graph_model.trim().to_string();
    let remote_embed_model = payload.remote_profile.embed_model.trim().to_string();

    if local_chat_model.is_empty()
        || local_graph_model.is_empty()
        || local_embed_model.is_empty()
        || remote_chat_model.is_empty()
        || remote_graph_model.is_empty()
        || remote_embed_model.is_empty()
    {
        return Err("chat/graph/embed 模型名均不能为空".to_string());
    }

    let local_models_root =
        normalize_optional_text(payload.local_profile.models_root).map(|path| {
            let p = PathBuf::from(&path);
            p.canonicalize().unwrap_or(p).to_string_lossy().to_string()
        });

    Ok(ModelSettingsDto {
        active_provider: provider_to_string(active_provider),
        local_profile: LocalModelProfileDto {
            endpoint: local_endpoint,
            models_root: local_models_root,
            chat_model: local_chat_model,
            graph_model: local_graph_model,
            embed_model: local_embed_model,
        },
        remote_profile: RemoteModelProfileDto {
            endpoint: remote_endpoint,
            api_key: normalize_optional_text(payload.remote_profile.api_key),
            chat_model: remote_chat_model,
            graph_model: remote_graph_model,
            embed_model: remote_embed_model,
        },
    })
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|v| {
        let trimmed = v.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn normalize_endpoint(provider: ModelProvider, endpoint: &str) -> String {
    let trimmed = endpoint.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }
    if provider == ModelProvider::OpenAiCompatible {
        memori_core::DEFAULT_MODEL_ENDPOINT_OPENAI.to_string()
    } else {
        DEFAULT_MODEL_ENDPOINT_OLLAMA.to_string()
    }
}

fn resolve_active_runtime_settings(settings: &ModelSettingsDto) -> ActiveRuntimeModelSettings {
    let active_provider = ModelProvider::from_value(&settings.active_provider);
    if active_provider == ModelProvider::OpenAiCompatible {
        return ActiveRuntimeModelSettings {
            provider: ModelProvider::OpenAiCompatible,
            endpoint: normalize_endpoint(
                ModelProvider::OpenAiCompatible,
                &settings.remote_profile.endpoint,
            ),
            api_key: normalize_optional_text(settings.remote_profile.api_key.clone()),
            models_root: None,
            chat_model: settings.remote_profile.chat_model.trim().to_string(),
            graph_model: settings.remote_profile.graph_model.trim().to_string(),
            embed_model: settings.remote_profile.embed_model.trim().to_string(),
        };
    }

    ActiveRuntimeModelSettings {
        provider: ModelProvider::OllamaLocal,
        endpoint: normalize_endpoint(ModelProvider::OllamaLocal, &settings.local_profile.endpoint),
        api_key: None,
        models_root: normalize_optional_text(settings.local_profile.models_root.clone()),
        chat_model: settings.local_profile.chat_model.trim().to_string(),
        graph_model: settings.local_profile.graph_model.trim().to_string(),
        embed_model: settings.local_profile.embed_model.trim().to_string(),
    }
}

fn apply_model_settings_to_env(settings: ActiveRuntimeModelSettings) {
    // SAFETY: process-global config source for memori-core runtime.
    unsafe {
        std::env::set_var(
            MEMORI_MODEL_PROVIDER_ENV,
            provider_to_string(settings.provider),
        );
        std::env::set_var(MEMORI_MODEL_ENDPOINT_ENV, &settings.endpoint);
        std::env::set_var(MEMORI_CHAT_MODEL_ENV, &settings.chat_model);
        std::env::set_var(MEMORI_GRAPH_MODEL_ENV, &settings.graph_model);
        std::env::set_var(MEMORI_EMBED_MODEL_ENV, &settings.embed_model);
        if let Some(key) = settings.api_key.as_ref() {
            std::env::set_var(MEMORI_MODEL_API_KEY_ENV, key);
        } else {
            std::env::remove_var(MEMORI_MODEL_API_KEY_ENV);
        }
    }
}

async fn fetch_provider_models(
    provider: ModelProvider,
    endpoint: &str,
    api_key: Option<&str>,
    models_root: Option<&str>,
) -> Result<ProviderModelsDto, ProviderModelFetchError> {
    match provider {
        ModelProvider::OllamaLocal => {
            let from_folder = models_root
                .map(PathBuf::from)
                .map(|root| scan_local_model_files_from_root(&root))
                .transpose()
                .map_err(|err| ProviderModelFetchError {
                    code: "models_root_invalid".to_string(),
                    message: err,
                })?
                .unwrap_or_default();
            let from_service = list_ollama_models(endpoint).await?;
            Ok(merge_model_candidates(from_folder, from_service))
        }
        ModelProvider::OpenAiCompatible => {
            let from_service = list_openai_compatible_models(endpoint, api_key).await?;
            Ok(merge_model_candidates(Vec::new(), from_service))
        }
    }
}

fn merge_model_candidates(
    from_folder: Vec<String>,
    from_service: Vec<String>,
) -> ProviderModelsDto {
    let mut merged_set = BTreeSet::new();
    for model in &from_folder {
        merged_set.insert(model.clone());
    }
    for model in &from_service {
        merged_set.insert(model.clone());
    }
    ProviderModelsDto {
        from_folder,
        from_service,
        merged: merged_set.into_iter().collect(),
    }
}

async fn list_ollama_models(endpoint: &str) -> Result<Vec<String>, ProviderModelFetchError> {
    #[derive(Debug, Deserialize)]
    struct OllamaTagResp {
        models: Vec<OllamaTagItem>,
    }
    #[derive(Debug, Deserialize)]
    struct OllamaTagItem {
        name: String,
    }
    let url = format!("{}/api/tags", endpoint.trim_end_matches('/'));
    let response = timeout(
        Duration::from_secs(PROVIDER_HTTP_TIMEOUT_SECS),
        reqwest::Client::new().get(url).send(),
    )
    .await
    .map_err(|_| ProviderModelFetchError {
        code: "request_timeout".to_string(),
        message: format!("连接 Ollama 超时({}s)", PROVIDER_HTTP_TIMEOUT_SECS),
    })?
    .map_err(|err| ProviderModelFetchError {
        code: "endpoint_unreachable".to_string(),
        message: format!("连接 Ollama 失败: {err}"),
    })?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(ProviderModelFetchError {
            code: "endpoint_unreachable".to_string(),
            message: format!("Ollama 模型列表请求失败: status={}, body={body}", status),
        });
    }
    let parsed: OllamaTagResp = response
        .json()
        .await
        .map_err(|err| ProviderModelFetchError {
            code: "endpoint_unreachable".to_string(),
            message: format!("解析 Ollama 模型列表失败: {err}"),
        })?;
    Ok(parsed.models.into_iter().map(|m| m.name).collect())
}

async fn list_openai_compatible_models(
    endpoint: &str,
    api_key: Option<&str>,
) -> Result<Vec<String>, ProviderModelFetchError> {
    #[derive(Debug, Deserialize)]
    struct OpenAiModelsResp {
        data: Vec<OpenAiModelItem>,
    }
    #[derive(Debug, Deserialize)]
    struct OpenAiModelItem {
        id: String,
    }
    let url = format!("{}/v1/models", endpoint.trim_end_matches('/'));
    let mut request = reqwest::Client::new().get(url);
    if let Some(key) = api_key {
        request = request.bearer_auth(key);
    }
    let response = timeout(
        Duration::from_secs(PROVIDER_HTTP_TIMEOUT_SECS),
        request.send(),
    )
    .await
    .map_err(|_| ProviderModelFetchError {
        code: "request_timeout".to_string(),
        message: format!("连接远程模型服务超时({}s)", PROVIDER_HTTP_TIMEOUT_SECS),
    })?
    .map_err(|err| ProviderModelFetchError {
        code: "endpoint_unreachable".to_string(),
        message: format!("连接远程模型服务失败: {err}"),
    })?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let code = if status.as_u16() == 401 || status.as_u16() == 403 {
            "auth_failed"
        } else {
            "endpoint_unreachable"
        };
        return Err(ProviderModelFetchError {
            code: code.to_string(),
            message: format!("status={}, body={body}", status),
        });
    }
    let parsed: OpenAiModelsResp =
        response
            .json()
            .await
            .map_err(|err| ProviderModelFetchError {
                code: "endpoint_unreachable".to_string(),
                message: format!("解析远程模型列表失败: {err}"),
            })?;
    Ok(parsed.data.into_iter().map(|m| m.id).collect())
}

fn scan_local_model_files_from_root(root: &Path) -> Result<Vec<String>, String> {
    if !root.exists() {
        return Err(format!("模型目录不存在: {}", root.display()));
    }
    if !root.is_dir() {
        return Err(format!("路径不是目录: {}", root.display()));
    }
    let mut set = BTreeSet::new();
    collect_local_model_files_recursive(root, &mut set, 0, 8)?;
    Ok(set.into_iter().collect())
}

fn collect_local_model_files_recursive(
    dir: &Path,
    set: &mut BTreeSet<String>,
    depth: usize,
    max_depth: usize,
) -> Result<(), String> {
    if depth > max_depth {
        return Ok(());
    }
    let entries =
        fs::read_dir(dir).map_err(|err| format!("读取模型目录失败({}): {err}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("读取模型目录项失败({}): {err}", dir.display()))?;
        let path = entry.path();
        let metadata = entry
            .metadata()
            .map_err(|err| format!("读取模型目录元数据失败({}): {err}", path.display()))?;
        if metadata.is_dir() {
            collect_local_model_files_recursive(&path, set, depth + 1, max_depth)?;
            continue;
        }
        if !metadata.is_file() {
            continue;
        }
        let Some(ext) = path.extension().and_then(|v| v.to_str()) else {
            continue;
        };
        if !ext.eq_ignore_ascii_case("gguf") {
            continue;
        }
        if let Some(name) = path.file_stem().and_then(|v| v.to_str()) {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                set.insert(trimmed.to_string());
            }
        }
    }
    Ok(())
}

async fn pull_ollama_model(
    endpoint: &str,
    model: &str,
    _api_key: Option<&str>,
) -> Result<(), String> {
    #[derive(Debug, Serialize)]
    struct PullBody<'a> {
        name: &'a str,
        stream: bool,
    }
    let url = format!("{}/api/pull", endpoint.trim_end_matches('/'));
    let response = timeout(
        Duration::from_secs(PROVIDER_HTTP_TIMEOUT_SECS),
        reqwest::Client::new()
            .post(url)
            .json(&PullBody {
                name: model,
                stream: false,
            })
            .send(),
    )
    .await
    .map_err(|_| format!("拉取模型超时({}s)", PROVIDER_HTTP_TIMEOUT_SECS))?
    .map_err(|err| format!("拉取模型失败: {err}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("拉取模型失败: status={}, body={body}", status));
    }
    Ok(())
}

fn model_exists(models: &[String], expected: &str) -> bool {
    let expected = expected.trim();
    if expected.is_empty() {
        return false;
    }
    models.iter().any(|m| m == expected)
        || (!expected.contains(':') && models.iter().any(|m| m == &format!("{expected}:latest")))
}

fn resolve_enterprise_policy(settings: &AppSettings) -> EnterprisePolicyDto {
    let indexing = resolve_indexing_config(settings);
    let allowed_model_endpoints = settings
        .enterprise_allowed_model_endpoints
        .clone()
        .unwrap_or_default()
        .into_iter()
        .map(|item| normalize_policy_endpoint(&item))
        .filter(|item| !item.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let allowed_models = settings
        .enterprise_allowed_models
        .clone()
        .unwrap_or_default()
        .into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    EnterprisePolicyDto {
        egress_mode: settings
            .enterprise_egress_mode
            .as_deref()
            .map(EgressMode::from_value)
            .unwrap_or_default(),
        allowed_model_endpoints,
        allowed_models,
        indexing_default_mode: indexing.mode.as_str().to_string(),
        resource_budget_default: indexing.resource_budget.as_str().to_string(),
        auth: AuthConfigDto {
            issuer: settings
                .oidc_issuer
                .clone()
                .unwrap_or_else(|| "https://example-idp.local".to_string()),
            client_id: settings
                .oidc_client_id
                .clone()
                .unwrap_or_else(|| "memori-vault-enterprise".to_string()),
            redirect_uri: settings
                .oidc_redirect_uri
                .clone()
                .unwrap_or_else(|| "http://localhost:3757/api/auth/oidc/login".to_string()),
            roles_claim: settings
                .oidc_roles_claim
                .clone()
                .unwrap_or_else(|| "roles".to_string()),
        },
    }
}

async fn require_session(
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

async fn resolve_actor_subject(state: &ServerState, headers: &HeaderMap) -> String {
    match require_session(state, headers, Role::Viewer).await {
        Ok(session) => session.subject,
        Err(_) => "anonymous".to_string(),
    }
}

async fn append_audit_event(state: &ServerState, event: AuditEventDto) {
    let _guard = state.audit_file_lock.lock().await;
    let path = match audit_log_file_path() {
        Ok(path) => path,
        Err(err) => {
            warn!(error = %err, "解析审计日志路径失败");
            return;
        }
    };
    if let Some(parent) = path.parent()
        && let Err(err) = fs::create_dir_all(parent)
    {
        warn!(error = %err, path = %parent.display(), "创建审计日志目录失败");
        return;
    }
    let line = match serde_json::to_string(&event) {
        Ok(line) => line,
        Err(err) => {
            warn!(error = %err, "序列化审计事件失败");
            return;
        }
    };
    let mut file = match fs::OpenOptions::new().create(true).append(true).open(&path) {
        Ok(file) => file,
        Err(err) => {
            warn!(error = %err, path = %path.display(), "打开审计日志文件失败");
            return;
        }
    };
    if let Err(err) = writeln!(file, "{line}") {
        warn!(error = %err, path = %path.display(), "写入审计日志失败");
    }
}

async fn append_policy_violation_audit(
    state: &ServerState,
    actor: String,
    action: &str,
    provider: Option<ModelProvider>,
    endpoint: Option<&str>,
    models: &[String],
    message: &str,
) {
    append_audit_event(
        state,
        AuditEventDto {
            actor,
            action: "policy_violation".to_string(),
            resource: "model_runtime".to_string(),
            timestamp: unix_now_secs(),
            result: "blocked".to_string(),
            metadata: serde_json::json!({
                "source_action": action,
                "provider": provider.map(provider_to_string),
                "endpoint": endpoint.map(|value| value.trim()),
                "models": models,
                "message": message,
            }),
        },
    )
    .await;
}

fn read_audit_events() -> Result<Vec<AuditEventDto>, String> {
    let path = audit_log_file_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = fs::File::open(&path)
        .map_err(|err| format!("打开审计日志失败({}): {err}", path.display()))?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|err| format!("读取审计日志失败({}): {err}", path.display()))?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(event) = serde_json::from_str::<AuditEventDto>(trimmed) {
            events.push(event);
        }
    }
    events.sort_by_key(|event| std::cmp::Reverse(event.timestamp));
    Ok(events)
}

fn decode_jwt_claims(token: &str) -> Result<serde_json::Value, String> {
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

fn extract_role_from_claims(value: &serde_json::Value) -> Option<Role> {
    match value {
        serde_json::Value::String(role) => Some(Role::from_value(role)),
        serde_json::Value::Array(items) => items
            .iter()
            .filter_map(|item| item.as_str().map(Role::from_value))
            .max(),
        _ => None,
    }
}

fn snapshot_metrics(metrics: &ServerMetrics) -> ServerMetricsDto {
    let ask_requests = metrics.ask_requests.load(Ordering::Relaxed);
    let ask_latency_total_ms = metrics.ask_latency_total_ms.load(Ordering::Relaxed);
    let ask_latency_avg_ms = if ask_requests == 0 {
        0.0
    } else {
        ask_latency_total_ms as f64 / ask_requests as f64
    };
    ServerMetricsDto {
        total_requests: metrics.total_requests.load(Ordering::Relaxed),
        failed_requests: metrics.failed_requests.load(Ordering::Relaxed),
        ask_requests,
        ask_failed: metrics.ask_failed.load(Ordering::Relaxed),
        ask_latency_avg_ms,
    }
}

fn unix_now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
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

fn audit_log_file_path() -> Result<PathBuf, String> {
    let config_root = dirs::config_dir().ok_or_else(|| "无法获取用户配置目录".to_string())?;
    Ok(config_root
        .join(SETTINGS_APP_DIR_NAME)
        .join(AUDIT_LOG_FILE_NAME))
}

fn normalize_top_k(top_k: Option<usize>) -> usize {
    match top_k {
        Some(value) if (1..=50).contains(&value) => value,
        _ => DEFAULT_RETRIEVE_TOP_K,
    }
}

fn normalize_language(lang: Option<&str>) -> Option<&'static str> {
    let lang = lang?;
    let lower = lang.trim().to_ascii_lowercase();
    if lower.starts_with("zh") {
        Some("zh-CN")
    } else if lower.starts_with("en") {
        Some("en-US")
    } else {
        None
    }
}

fn resolve_indexing_config(settings: &AppSettings) -> IndexingConfig {
    let mode = settings
        .indexing_mode
        .as_deref()
        .map(IndexingMode::from_value)
        .unwrap_or(IndexingMode::Continuous);
    let resource_budget = settings
        .resource_budget
        .as_deref()
        .map(ResourceBudget::from_value)
        .unwrap_or(ResourceBudget::Low);
    let schedule_window = if mode == IndexingMode::Scheduled {
        Some(ScheduleWindow {
            start: settings
                .schedule_start
                .clone()
                .unwrap_or_else(|| "00:00".to_string()),
            end: settings
                .schedule_end
                .clone()
                .unwrap_or_else(|| "06:00".to_string()),
        })
    } else {
        None
    };
    IndexingConfig {
        mode,
        resource_budget,
        schedule_window,
    }
}

fn chrono_like_now_token() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
        .unwrap_or(0);
    now.to_string()
}

fn normalize_scope_paths(scope_paths: Option<Vec<String>>) -> Vec<PathBuf> {
    let mut result = Vec::new();
    for scope in scope_paths.unwrap_or_default() {
        let trimmed = scope.trim();
        if trimmed.is_empty() {
            continue;
        }
        result.push(PathBuf::from(trimmed));
    }
    result
}

fn format_legacy_answer(response: &AskResponseStructured) -> String {
    match response.status {
        AskStatus::Answered => {
            let references = format_legacy_references(response);
            if references.is_empty() {
                response.answer.clone()
            } else {
                format!("{}\n\n---\n参考来源：\n{}", response.answer, references)
            }
        }
        AskStatus::InsufficientEvidence => "证据不足，当前无法可靠回答这个问题。".to_string(),
        AskStatus::ModelFailedWithEvidence => {
            let references = format_legacy_references(response);
            if references.is_empty() {
                "本地大模型答案生成失败，且没有可展示的证据片段。".to_string()
            } else {
                format!("本地大模型答案生成失败，以下是检索到的相关片段：\n\n{references}")
            }
        }
    }
}

fn format_legacy_references(response: &AskResponseStructured) -> String {
    let mut lines = Vec::new();
    for (index, evidence) in response.evidence.iter().enumerate() {
        lines.push(format!(
            "#{}  命中原因: {}  文档排序: #{}  片段排序: #{}",
            index + 1,
            evidence.reason,
            evidence.document_rank,
            evidence.chunk_rank
        ));
        lines.push(format!("来源: {}", evidence.file_path));
        lines.push(format!("块序号: {}", evidence.chunk_index));
        lines.push(evidence.content.clone());
        lines.push(String::from(
            "------------------------------------------------------------",
        ));
    }
    lines.join("\n")
}
