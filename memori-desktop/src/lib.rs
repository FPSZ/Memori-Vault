use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use memori_core::{
    AskResponseStructured, AskStatus, DEFAULT_CHAT_MODEL, DEFAULT_GRAPH_MODEL,
    DEFAULT_MODEL_ENDPOINT_OLLAMA, DEFAULT_MODEL_PROVIDER, DEFAULT_OLLAMA_EMBED_MODEL, EgressMode,
    EngineError, EnterpriseModelPolicy, IndexingConfig, IndexingMode, IndexingStatus,
    MEMORI_CHAT_MODEL_ENV, MEMORI_EMBED_MODEL_ENV, MEMORI_GRAPH_MODEL_ENV,
    MEMORI_MODEL_API_KEY_ENV, MEMORI_MODEL_ENDPOINT_ENV, MEMORI_MODEL_PROVIDER_ENV, MemoriEngine,
    ModelProvider, ResourceBudget, RuntimeModelConfig, ScheduleWindow, VaultStats,
    normalize_policy_endpoint, validate_provider_request, validate_runtime_model_settings,
};
use serde::{Deserialize, Serialize};
use tauri::{Manager, State, WindowEvent};
use tokio::sync::Mutex;
use tokio::time::{Duration, timeout};
use tracing::{error, info, warn};

const ENGINE_SHUTDOWN_TIMEOUT_SECS: u64 = 8;
const PROVIDER_HTTP_TIMEOUT_SECS: u64 = 15;
const SETTINGS_APP_DIR_NAME: &str = "Memori-Vault";
const SETTINGS_FILE_NAME: &str = "settings.json";
const DEFAULT_WINDOW_WIDTH: u32 = 1480;
const DEFAULT_WINDOW_HEIGHT: u32 = 920;
const MIN_WINDOW_WIDTH: u32 = 900;
const MIN_WINDOW_HEIGHT: u32 = 620;
const MODEL_NOT_CONFIGURED_CODE: &str = "model_not_configured";
const MODEL_NOT_CONFIGURED_MESSAGE: &str = "未配置模型，请在 设置 > 模型 中配置";

mod desktop_state;
mod dto;
mod model_runtime;
mod provider_client;
mod settings_io;
mod window_state;

mod commands;

pub(crate) use commands::*;
pub(crate) use desktop_state::*;
pub(crate) use dto::*;
pub(crate) use model_runtime::*;
pub(crate) use provider_client::*;
pub(crate) use settings_io::*;
pub(crate) use window_state::*;

pub fn run() {
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

    let shared_engine = Arc::new(Mutex::new(None));
    let daemon_engine = Arc::clone(&shared_engine);
    let init_error = Arc::new(Mutex::new(None));
    let init_error_worker = Arc::clone(&init_error);

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            if let Some(main_window) = app.get_webview_window("main")
                && let Err(err) = restore_main_window_state(&main_window, &settings)
            {
                warn!(error = %err, "恢复窗口状态失败，已回退默认窗口布局");
            }

            let daemon_watch_root = watch_root.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(err) = replace_engine(
                    &daemon_engine,
                    &init_error_worker,
                    daemon_watch_root.clone(),
                    "setup_bootstrap",
                )
                .await
                {
                    error!(error = %err, "memori-desktop daemon bootstrap failed in setup");
                }
            });

            // Set window icon explicitly for decorations:false (taskbar icon)
            if let Some(main_window) = app.get_webview_window("main") {
                match tauri::image::Image::from_bytes(include_bytes!("../icons/icon.png")) {
                    Ok(icon) => {
                        if let Err(err) = main_window.set_icon(icon) {
                            warn!(error = %err, "设置窗口图标失败");
                        }
                    }
                    Err(err) => {
                        warn!(error = %err, "加载窗口图标失败");
                    }
                }
            }

            app.manage(DesktopState {
                engine: Arc::clone(&shared_engine),
                init_error: Arc::clone(&init_error),
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            match event {
                WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
                    if let Err(err) = persist_main_window_state(window) {
                        warn!(error = %err, "持久化窗口状态失败");
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            ask_vault_structured,
            ask_vault,
            get_vault_stats,
            get_indexing_status,
            set_indexing_mode,
            trigger_reindex,
            pause_indexing,
            resume_indexing,
            get_app_settings,
            get_model_settings,
            get_enterprise_policy,
            set_enterprise_policy,
            set_model_settings,
            list_provider_models,
            probe_model_provider,
            validate_model_setup,
            pull_model,
            set_local_models_root,
            scan_local_model_files,
            set_watch_root,
            list_search_scopes,
            open_source_location,
            rank_settings_query
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|err| {
            error!(error = %err, "tauri runtime exited with error");
        });
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

fn collect_search_scopes(root: &std::path::Path) -> Result<Vec<SearchScopeItem>, String> {
    const MAX_SCOPE_ITEMS: usize = 20000;

    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut result = Vec::new();
    collect_search_scopes_recursive(root, root, 0, &mut result, MAX_SCOPE_ITEMS);

    Ok(result)
}

fn collect_search_scopes_recursive(
    root: &std::path::Path,
    current_dir: &std::path::Path,
    depth: usize,
    result: &mut Vec<SearchScopeItem>,
    max_items: usize,
) {
    if result.len() >= max_items {
        return;
    }

    let entries = match std::fs::read_dir(current_dir) {
        Ok(entries) => entries,
        Err(err) => {
            warn!(path = %current_dir.display(), error = %err, "读取目录失败，已跳过");
            return;
        }
    };

    let mut collected = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                warn!(path = %current_dir.display(), error = %err, "读取目录项失败，已跳过");
                continue;
            }
        };
        collected.push(entry.path());
    }

    collected.sort_by(|a, b| {
        let a_is_dir = a.is_dir();
        let b_is_dir = b.is_dir();
        b_is_dir
            .cmp(&a_is_dir)
            .then_with(|| a.file_name().cmp(&b.file_name()))
    });

    for path in collected {
        if result.len() >= max_items {
            return;
        }

        let metadata = match std::fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(err) => {
                warn!(path = %path.display(), error = %err, "读取路径元数据失败，已跳过");
                continue;
            }
        };

        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        let relative_path = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();

        if metadata.is_dir() {
            result.push(SearchScopeItem {
                path: path.to_string_lossy().to_string(),
                name,
                relative_path,
                is_dir: true,
                depth,
            });
            collect_search_scopes_recursive(root, &path, depth + 1, result, max_items);
            continue;
        }

        if metadata.is_file() && is_supported_text_file(&path) {
            result.push(SearchScopeItem {
                path: path.to_string_lossy().to_string(),
                name,
                relative_path,
                is_dir: false,
                depth,
            });
        }
    }
}

fn is_supported_text_file(path: &std::path::Path) -> bool {
    let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
        return false;
    };
    ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("txt")
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
    for evidence in &response.evidence {
        lines.push(format!(
            "#{}  命中原因: {}  文档排序: #{}  片段排序: #{}",
            lines.len() / 5 + 1,
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
