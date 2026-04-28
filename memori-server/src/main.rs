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
    AskResponseStructured, AskStatus, DEFAULT_CHAT_ENDPOINT, DEFAULT_CHAT_MODEL,
    DEFAULT_EMBED_MODEL_QWEN3, DEFAULT_GRAPH_MODEL, DEFAULT_MODEL_PROVIDER, EgressMode,
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

mod audit;
mod auth;
mod dto;
mod mcp;
mod model_runtime;
mod routes;
mod settings_io;
mod state;

pub(crate) use audit::*;
pub(crate) use auth::*;
pub(crate) use dto::*;
// mcp types are accessed via crate::mcp:: path, no need for glob re-export
pub(crate) use model_runtime::*;
pub(crate) use routes::*;
pub(crate) use settings_io::*;
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
    let log_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Memori-Vault")
        .join("logs");
    let _log_guard = memori_core::logging::init_logging(log_dir);

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
    if std::env::args().any(|arg| arg == "--mcp-stdio") {
        if let Err(err) = mcp::transport_stdio::run_stdio_server(app_state).await {
            error!(error = %err, "memori-server MCP stdio exited with error");
        }
        return;
    }

    let app = build_router(app_state);

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
