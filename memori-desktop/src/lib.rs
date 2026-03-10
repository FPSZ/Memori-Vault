use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use memori_core::{
    DEFAULT_CHAT_MODEL, DEFAULT_GRAPH_MODEL, DEFAULT_MODEL_ENDPOINT_OLLAMA, DEFAULT_MODEL_PROVIDER,
    DEFAULT_OLLAMA_EMBED_MODEL, DocumentChunk, IndexingConfig, IndexingMode, IndexingStatus,
    MEMORI_CHAT_MODEL_ENV, MEMORI_EMBED_MODEL_ENV, MEMORI_GRAPH_MODEL_ENV,
    MEMORI_MODEL_API_KEY_ENV, MEMORI_MODEL_ENDPOINT_ENV, MEMORI_MODEL_PROVIDER_ENV, MemoriEngine,
    ModelProvider, ResourceBudget, ScheduleWindow, VaultStats,
};
use serde::{Deserialize, Serialize};
use tauri::{Manager, PhysicalPosition, PhysicalSize, Position, Size, State, WindowEvent};
use tokio::sync::Mutex;
use tokio::time::{Duration, timeout};
use tracing::{error, info, warn};

const DEFAULT_RETRIEVE_TOP_K: usize = 20;
const ENGINE_SHUTDOWN_TIMEOUT_SECS: u64 = 8;
const PROVIDER_HTTP_TIMEOUT_SECS: u64 = 15;
const SETTINGS_APP_DIR_NAME: &str = "Memori-Vault";
const SETTINGS_FILE_NAME: &str = "settings.json";
const DEFAULT_WINDOW_WIDTH: u32 = 1480;
const DEFAULT_WINDOW_HEIGHT: u32 = 920;
const MIN_WINDOW_WIDTH: u32 = 900;
const MIN_WINDOW_HEIGHT: u32 = 620;

struct DesktopState {
    engine: Arc<Mutex<Option<MemoriEngine>>>,
    init_error: Arc<Mutex<Option<String>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct AppSettings {
    watch_root: Option<String>,
    language: Option<String>,
    indexing_mode: Option<String>,
    resource_budget: Option<String>,
    schedule_start: Option<String>,
    schedule_end: Option<String>,
    active_provider: Option<String>,
    local_endpoint: Option<String>,
    local_models_root: Option<String>,
    local_chat_model: Option<String>,
    local_graph_model: Option<String>,
    local_embed_model: Option<String>,
    remote_endpoint: Option<String>,
    remote_api_key: Option<String>,
    remote_chat_model: Option<String>,
    remote_graph_model: Option<String>,
    remote_embed_model: Option<String>,
    window_x: Option<i32>,
    window_y: Option<i32>,
    window_width: Option<u32>,
    window_height: Option<u32>,
    window_maximized: Option<bool>,
    // legacy fields for backwards compatibility
    provider: Option<String>,
    endpoint: Option<String>,
    api_key: Option<String>,
    chat_model: Option<String>,
    graph_model: Option<String>,
    embed_model: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct AppSettingsDto {
    watch_root: String,
    language: Option<String>,
    indexing_mode: String,
    resource_budget: String,
    schedule_start: Option<String>,
    schedule_end: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SetIndexingModePayload {
    indexing_mode: String,
    resource_budget: String,
    schedule_start: Option<String>,
    schedule_end: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LocalModelProfileDto {
    endpoint: String,
    models_root: Option<String>,
    chat_model: String,
    graph_model: String,
    embed_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RemoteModelProfileDto {
    endpoint: String,
    api_key: Option<String>,
    chat_model: String,
    graph_model: String,
    embed_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ModelSettingsDto {
    active_provider: String,
    local_profile: LocalModelProfileDto,
    remote_profile: RemoteModelProfileDto,
}

#[derive(Debug, Clone, Serialize)]
struct ModelErrorItem {
    code: String,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
struct ModelAvailabilityDto {
    reachable: bool,
    models: Vec<String>,
    missing_roles: Vec<String>,
    errors: Vec<ModelErrorItem>,
    checked_provider: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ProviderModelsDto {
    from_folder: Vec<String>,
    from_service: Vec<String>,
    merged: Vec<String>,
}

#[derive(Debug, Clone)]
struct ProviderModelFetchError {
    code: String,
    message: String,
}

#[derive(Debug, Clone, Deserialize)]
struct SettingsSearchCandidate {
    key: String,
    text: String,
}

#[derive(Debug, Clone, Serialize)]
struct SearchScopeItem {
    path: String,
    name: String,
    relative_path: String,
    is_dir: bool,
    depth: usize,
}

#[tauri::command]
async fn ask_vault(
    query: String,
    lang: Option<String>,
    top_k: Option<usize>,
    scope_paths: Option<Vec<String>>,
    state: State<'_, DesktopState>,
) -> Result<String, String> {
    let query = query.trim().to_string();
    if query.is_empty() {
        return Ok("请输入一个非空问题。".to_string());
    }

    let engine_guard = state.engine.lock().await;
    let init_error_guard = state.init_error.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_guard.as_ref() {
            return Err(format!("引擎初始化失败: {message}"));
        }
        return Err("引擎尚在初始化中，请稍后重试。".to_string());
    };

    let top_k = normalize_top_k(top_k);
    let mut scope_paths = normalize_scope_paths(scope_paths);
    if scope_paths.is_empty()
        && let Ok(settings) = load_app_settings()
        && let Ok(watch_root) = resolve_watch_root_from_settings(&settings)
    {
        scope_paths.push(watch_root);
    }
    let scope_refs = if scope_paths.is_empty() {
        None
    } else {
        Some(scope_paths.as_slice())
    };

    let results = engine
        .search(&query, top_k, scope_refs)
        .await
        .map_err(|err| err.to_string())?;
    if results.is_empty() {
        return Ok("未检索到相关记忆。".to_string());
    }

    let text_context = build_text_context(&results);
    let graph_context = match engine.get_graph_context_for_results(&results).await {
        Ok(context) => context,
        Err(err) => {
            warn!(error = %err, "图谱上下文构建失败，降级为纯文本上下文回答");
            String::new()
        }
    };

    let answer_question = build_answer_question(&query, lang.as_deref());
    let references = format_references(&results);
    match engine
        .generate_answer(&answer_question, &text_context, &graph_context)
        .await
    {
        Ok(answer) => Ok(format!("{answer}\n\n---\n参考来源：\n{references}")),
        Err(err) => {
            warn!(error = %err, "答案合成失败，降级返回向量检索结果");
            Ok(format!(
                "本地大模型答案生成失败，以下是检索到的相关片段：\n\n{references}"
            ))
        }
    }
}

#[tauri::command]
async fn get_vault_stats(state: State<'_, DesktopState>) -> Result<VaultStats, String> {
    let engine_guard = state.engine.lock().await;
    let init_error_guard = state.init_error.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_guard.as_ref() {
            return Err(format!("引擎初始化失败: {message}"));
        }
        return Err("引擎尚在初始化中，请稍后重试。".to_string());
    };
    engine
        .get_vault_stats()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn get_indexing_status(state: State<'_, DesktopState>) -> Result<IndexingStatus, String> {
    let engine_guard = state.engine.lock().await;
    let init_error_guard = state.init_error.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_guard.as_ref() {
            return Err(format!("引擎初始化失败: {message}"));
        }
        return Err("引擎尚在初始化中，请稍后重试。".to_string());
    };
    engine
        .get_indexing_status()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn set_indexing_mode(
    payload: SetIndexingModePayload,
    state: State<'_, DesktopState>,
) -> Result<AppSettingsDto, String> {
    let mut settings = load_app_settings()?;
    let mode = IndexingMode::from_value(&payload.indexing_mode);
    let budget = ResourceBudget::from_value(&payload.resource_budget);
    let schedule_window = if mode == IndexingMode::Scheduled {
        let start = payload
            .schedule_start
            .unwrap_or_else(|| "00:00".to_string())
            .trim()
            .to_string();
        let end = payload
            .schedule_end
            .unwrap_or_else(|| "06:00".to_string())
            .trim()
            .to_string();
        Some(ScheduleWindow { start, end })
    } else {
        None
    };
    settings.indexing_mode = Some(mode.as_str().to_string());
    settings.resource_budget = Some(budget.as_str().to_string());
    settings.schedule_start = schedule_window.as_ref().map(|w| w.start.clone());
    settings.schedule_end = schedule_window.as_ref().map(|w| w.end.clone());
    save_app_settings(&settings)?;

    let engine_guard = state.engine.lock().await;
    let init_error_guard = state.init_error.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_guard.as_ref() {
            return Err(format!("引擎初始化失败: {message}"));
        }
        return Err("引擎尚在初始化中，请稍后重试。".to_string());
    };

    engine
        .set_indexing_config(IndexingConfig {
            mode,
            resource_budget: budget,
            schedule_window,
        })
        .await;

    let watch_root = resolve_watch_root_from_settings(&settings)?;
    let indexing = resolve_indexing_config(&settings);
    Ok(AppSettingsDto {
        watch_root: watch_root.to_string_lossy().to_string(),
        language: settings.language,
        indexing_mode: indexing.mode.as_str().to_string(),
        resource_budget: indexing.resource_budget.as_str().to_string(),
        schedule_start: indexing.schedule_window.as_ref().map(|w| w.start.clone()),
        schedule_end: indexing.schedule_window.as_ref().map(|w| w.end.clone()),
    })
}

#[tauri::command]
async fn trigger_reindex(state: State<'_, DesktopState>) -> Result<String, String> {
    let task_id = format!("reindex-{}", chrono_like_now_token());
    let engine_guard = state.engine.lock().await;
    let init_error_guard = state.init_error.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_guard.as_ref() {
            return Err(format!("引擎初始化失败: {message}"));
        }
        return Err("引擎尚在初始化中，请稍后重试。".to_string());
    };
    engine
        .trigger_reindex()
        .await
        .map_err(|err| err.to_string())?;
    Ok(task_id)
}

#[tauri::command]
async fn pause_indexing(state: State<'_, DesktopState>) -> Result<(), String> {
    let engine_guard = state.engine.lock().await;
    let init_error_guard = state.init_error.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_guard.as_ref() {
            return Err(format!("引擎初始化失败: {message}"));
        }
        return Err("引擎尚在初始化中，请稍后重试。".to_string());
    };
    engine.pause_indexing().await;
    Ok(())
}

#[tauri::command]
async fn resume_indexing(state: State<'_, DesktopState>) -> Result<(), String> {
    let engine_guard = state.engine.lock().await;
    let init_error_guard = state.init_error.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_guard.as_ref() {
            return Err(format!("引擎初始化失败: {message}"));
        }
        return Err("引擎尚在初始化中，请稍后重试。".to_string());
    };
    engine.resume_indexing().await;
    Ok(())
}

#[tauri::command]
async fn get_app_settings() -> Result<AppSettingsDto, String> {
    let settings = load_app_settings()?;
    let watch_root = resolve_watch_root_from_settings(&settings)?;
    let indexing = resolve_indexing_config(&settings);
    Ok(AppSettingsDto {
        watch_root: watch_root.to_string_lossy().to_string(),
        language: settings.language,
        indexing_mode: indexing.mode.as_str().to_string(),
        resource_budget: indexing.resource_budget.as_str().to_string(),
        schedule_start: indexing.schedule_window.as_ref().map(|w| w.start.clone()),
        schedule_end: indexing.schedule_window.as_ref().map(|w| w.end.clone()),
    })
}

#[tauri::command]
async fn get_model_settings() -> Result<ModelSettingsDto, String> {
    let settings = load_app_settings()?;
    Ok(resolve_model_settings(&settings))
}

#[tauri::command]
async fn set_model_settings(
    payload: ModelSettingsDto,
    state: State<'_, DesktopState>,
) -> Result<ModelSettingsDto, String> {
    let mut settings = load_app_settings()?;
    let normalized = normalize_model_settings_payload(payload)?;
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
    save_app_settings(&settings)?;
    apply_model_settings_to_env(resolve_active_runtime_settings(&normalized));

    let watch_root = resolve_watch_root_from_settings(&settings)?;
    replace_engine(
        &state.engine,
        &state.init_error,
        watch_root,
        "settings_model_update",
    )
    .await?;

    Ok(normalized)
}

#[tauri::command]
async fn list_provider_models(
    provider: String,
    endpoint: String,
    api_key: Option<String>,
    models_root: Option<String>,
) -> Result<ProviderModelsDto, String> {
    let provider = ModelProvider::from_value(&provider);
    let endpoint = normalize_endpoint(provider, &endpoint);
    let api_key = normalize_optional_text(api_key);
    let models_root = normalize_optional_text(models_root);
    if provider == ModelProvider::OllamaLocal {
        let from_folder = models_root
            .as_deref()
            .map(PathBuf::from)
            .map(|root| scan_local_model_files_from_root(&root))
            .transpose()
            .map_err(|err| format!("models_root_invalid: {err}"))?
            .unwrap_or_default();
        let from_service = list_ollama_models(&endpoint).await.unwrap_or_default();
        return Ok(merge_model_candidates(from_folder, from_service));
    }
    fetch_provider_models(
        provider,
        &endpoint,
        api_key.as_deref(),
        models_root.as_deref(),
    )
    .await
    .map_err(|err| format!("{}: {}", err.code, err.message))
}

#[tauri::command]
async fn probe_model_provider(
    provider: String,
    endpoint: String,
    api_key: Option<String>,
    models_root: Option<String>,
) -> Result<ModelAvailabilityDto, String> {
    let provider = ModelProvider::from_value(&provider);
    let endpoint = normalize_endpoint(provider, &endpoint);
    let api_key = normalize_optional_text(api_key);
    let models_root = normalize_optional_text(models_root);
    let result = fetch_provider_models(
        provider,
        &endpoint,
        api_key.as_deref(),
        models_root.as_deref(),
    )
    .await;
    match result {
        Ok(models) => Ok(ModelAvailabilityDto {
            reachable: true,
            models: models.merged,
            missing_roles: Vec::new(),
            errors: Vec::new(),
            checked_provider: Some(provider_to_string(provider)),
        }),
        Err(err) => Ok(ModelAvailabilityDto {
            reachable: false,
            models: Vec::new(),
            missing_roles: Vec::new(),
            errors: vec![ModelErrorItem {
                code: err.code,
                message: err.message,
            }],
            checked_provider: Some(provider_to_string(provider)),
        }),
    }
}

#[tauri::command]
async fn validate_model_setup() -> Result<ModelAvailabilityDto, String> {
    let settings = load_app_settings()?;
    let model_settings = resolve_model_settings(&settings);
    let active = resolve_active_runtime_settings(&model_settings);
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

            Ok(ModelAvailabilityDto {
                reachable: true,
                models: merged,
                missing_roles,
                errors: Vec::new(),
                checked_provider: Some(provider_to_string(provider)),
            })
        }
        Err(err) => Ok(ModelAvailabilityDto {
            reachable: false,
            models: Vec::new(),
            missing_roles: vec!["chat".to_string(), "graph".to_string(), "embed".to_string()],
            errors: vec![ModelErrorItem {
                code: err.code,
                message: err.message,
            }],
            checked_provider: Some(provider_to_string(provider)),
        }),
    }
}

#[tauri::command]
async fn pull_model(
    model: String,
    provider: String,
    endpoint: String,
    api_key: Option<String>,
) -> Result<ModelAvailabilityDto, String> {
    let model = model.trim().to_string();
    if model.is_empty() {
        return Err("模型名不能为空".to_string());
    }
    let provider = ModelProvider::from_value(&provider);
    if provider != ModelProvider::OllamaLocal {
        return Err("仅本地 Ollama 模式支持拉取模型".to_string());
    }
    let endpoint = normalize_endpoint(provider, &endpoint);
    let api_key = normalize_optional_text(api_key);
    pull_ollama_model(&endpoint, &model, api_key.as_deref()).await?;
    validate_model_setup().await
}

#[tauri::command]
async fn set_local_models_root(path: String) -> Result<ModelSettingsDto, String> {
    let mut settings = load_app_settings()?;
    let root = normalize_optional_text(Some(path));
    if let Some(root_path) = root.as_deref() {
        let path = PathBuf::from(root_path);
        if !path.exists() {
            return Err(format!("模型目录不存在: {}", path.display()));
        }
        if !path.is_dir() {
            return Err(format!("路径不是目录: {}", path.display()));
        }
        settings.local_models_root = Some(
            path.canonicalize()
                .unwrap_or(path)
                .to_string_lossy()
                .to_string(),
        );
    } else {
        settings.local_models_root = None;
    }
    save_app_settings(&settings)?;
    Ok(resolve_model_settings(&settings))
}

#[tauri::command]
async fn scan_local_model_files(root: Option<String>) -> Result<Vec<String>, String> {
    let root = normalize_optional_text(root);
    if let Some(root) = root {
        return scan_local_model_files_from_root(&PathBuf::from(root));
    }
    let settings = load_app_settings()?;
    let resolved = resolve_model_settings(&settings);
    let Some(root) = resolved.local_profile.models_root else {
        return Ok(Vec::new());
    };
    scan_local_model_files_from_root(&PathBuf::from(root))
}

#[tauri::command]
async fn set_watch_root(
    path: String,
    state: State<'_, DesktopState>,
) -> Result<AppSettingsDto, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("目录路径为空，无法保存。".to_string());
    }

    let watch_root = PathBuf::from(trimmed);
    if !watch_root.exists() {
        return Err(format!("目录不存在: {}", watch_root.display()));
    }
    if !watch_root.is_dir() {
        return Err(format!("路径不是目录: {}", watch_root.display()));
    }

    let canonical = watch_root
        .canonicalize()
        .map_err(|err| format!("规范化目录失败: {err}"))?;

    let mut settings = load_app_settings()?;
    settings.watch_root = Some(canonical.to_string_lossy().to_string());
    save_app_settings(&settings)?;

    replace_engine(
        &state.engine,
        &state.init_error,
        canonical.clone(),
        "settings_watch_root_update",
    )
    .await?;

    let indexing = resolve_indexing_config(&settings);
    Ok(AppSettingsDto {
        watch_root: canonical.to_string_lossy().to_string(),
        language: settings.language,
        indexing_mode: indexing.mode.as_str().to_string(),
        resource_budget: indexing.resource_budget.as_str().to_string(),
        schedule_start: indexing.schedule_window.as_ref().map(|w| w.start.clone()),
        schedule_end: indexing.schedule_window.as_ref().map(|w| w.end.clone()),
    })
}

#[tauri::command]
async fn list_search_scopes() -> Result<Vec<SearchScopeItem>, String> {
    let settings = load_app_settings()?;
    let watch_root = resolve_watch_root_from_settings(&settings)?;
    collect_search_scopes(&watch_root)
}

#[tauri::command]
async fn open_source_location(path: String) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("文件路径为空，无法打开。".to_string());
    }

    let target = PathBuf::from(trimmed);
    if !target.exists() {
        return Err(format!("文件不存在: {}", target.display()));
    }

    #[cfg(target_os = "windows")]
    {
        let canonical = target.canonicalize().unwrap_or_else(|_| target.clone());
        let normalized = canonical.to_string_lossy().replace('/', "\\");
        if canonical.is_file() {
            if let Err(first_err) = Command::new("explorer.exe")
                .arg("/select,")
                .arg(&normalized)
                .spawn()
            {
                Command::new("explorer.exe")
                    .arg(format!("/select,\"{normalized}\""))
                    .spawn()
                    .map_err(|fallback_err| {
                        format!("打开文件位置失败: {first_err}; fallback: {fallback_err}")
                    })?;
            }
        } else {
            Command::new("explorer.exe")
                .arg(&normalized)
                .spawn()
                .map_err(|err| format!("打开文件位置失败: {err}"))?;
        }
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        let status = Command::new("open")
            .arg("-R")
            .arg(&target)
            .status()
            .map_err(|err| format!("打开文件位置失败: {err}"))?;
        if !status.success() {
            return Err("打开文件位置失败: open 返回非零状态".to_string());
        }
        return Ok(());
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let open_path = if target.is_file() {
            target
                .parent()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
        } else {
            target
        };
        let status = Command::new("xdg-open")
            .arg(open_path)
            .status()
            .map_err(|err| format!("打开文件位置失败: {err}"))?;
        if !status.success() {
            return Err("打开文件位置失败: xdg-open 返回非零状态".to_string());
        }
        return Ok(());
    }

    #[allow(unreachable_code)]
    Err("当前系统暂不支持打开文件位置".to_string())
}

#[tauri::command]
async fn rank_settings_query(
    query: String,
    candidates: Vec<SettingsSearchCandidate>,
    lang: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<Vec<String>, String> {
    let query = query.trim();
    if query.is_empty() || candidates.is_empty() {
        return Ok(Vec::new());
    }

    let engine_guard = state.engine.lock().await;
    let init_error_guard = state.init_error.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_guard.as_ref() {
            return Err(format!("引擎初始化失败: {message}"));
        }
        return Err("引擎尚在初始化中，请稍后重试。".to_string());
    };

    let mut candidate_lines = Vec::with_capacity(candidates.len());
    for item in &candidates {
        candidate_lines.push(format!("{} => {}", item.key.trim(), item.text.trim()));
    }

    let prompt = match normalize_language(lang.as_deref()) {
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
        .map_err(|err| err.to_string())?;

    let candidate_keys: std::collections::HashSet<String> = candidates
        .iter()
        .map(|c| c.key.trim().to_string())
        .collect();

    if let Ok(parsed) = serde_json::from_str::<Vec<String>>(&answer) {
        let matched = parsed
            .into_iter()
            .filter(|key| candidate_keys.contains(key.trim()))
            .collect::<Vec<_>>();
        if !matched.is_empty() {
            return Ok(matched);
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
                return Ok(matched);
            }
        }
    }

    let lower_answer = answer.to_ascii_lowercase();
    let fallback = candidates
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

    Ok(fallback)
}

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
    let model_settings = resolve_model_settings(&settings);
    apply_model_settings_to_env(resolve_active_runtime_settings(&model_settings));

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
            ask_vault,
            get_vault_stats,
            get_indexing_status,
            set_indexing_mode,
            trigger_reindex,
            pause_indexing,
            resume_indexing,
            get_app_settings,
            get_model_settings,
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

    let mut new_engine =
        MemoriEngine::bootstrap(watch_root.clone()).map_err(|err| err.to_string())?;
    if let Ok(settings) = load_app_settings() {
        new_engine
            .set_indexing_config(resolve_indexing_config(&settings))
            .await;
    }
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
        "memori-desktop daemon started"
    );

    Ok(())
}

fn app_settings_file_path() -> Result<PathBuf, String> {
    let config_root = dirs::config_dir().ok_or_else(|| "无法获取用户配置目录".to_string())?;
    Ok(config_root
        .join(SETTINGS_APP_DIR_NAME)
        .join(SETTINGS_FILE_NAME))
}

fn restore_main_window_state(
    window: &tauri::WebviewWindow,
    settings: &AppSettings,
) -> Result<(), String> {
    let has_saved_size = match (settings.window_width, settings.window_height) {
        (Some(w), Some(h)) => w >= MIN_WINDOW_WIDTH && h >= MIN_WINDOW_HEIGHT,
        _ => false,
    };
    let has_saved_position = match (settings.window_x, settings.window_y) {
        (Some(x), Some(y)) => x > -10_000 && y > -10_000,
        _ => false,
    };

    let monitor = window
        .current_monitor()
        .map_err(|err| format!("读取当前显示器失败: {err}"))?
        .or_else(|| window.primary_monitor().ok().flatten());

    let Some(monitor) = monitor else {
        let target_w = settings
            .window_width
            .unwrap_or(DEFAULT_WINDOW_WIDTH)
            .max(MIN_WINDOW_WIDTH);
        let target_h = settings
            .window_height
            .unwrap_or(DEFAULT_WINDOW_HEIGHT)
            .max(MIN_WINDOW_HEIGHT);
        window
            .set_size(Size::Physical(PhysicalSize::new(target_w, target_h)))
            .map_err(|err| format!("设置窗口尺寸失败: {err}"))?;
        if has_saved_position {
            if let (Some(x), Some(y)) = (settings.window_x, settings.window_y) {
                let _ = window.set_position(Position::Physical(PhysicalPosition::new(x, y)));
            }
        } else {
            let _ = window.center();
        }
        if settings.window_maximized.unwrap_or(false) {
            let _ = window.maximize();
        }
        return Ok(());
    };

    let monitor_pos = monitor.position();
    let monitor_size = monitor.size();
    let max_w = monitor_size.width.saturating_sub(32);
    let max_h = monitor_size.height.saturating_sub(64);
    let default_w = ((monitor_size.width as f32) * 0.9).round() as u32;
    let default_h = ((monitor_size.height as f32) * 0.88).round() as u32;

    let target_w = settings
        .window_width
        .unwrap_or_else(|| DEFAULT_WINDOW_WIDTH.max(default_w))
        .clamp(MIN_WINDOW_WIDTH, max_w.max(MIN_WINDOW_WIDTH));
    let target_h = settings
        .window_height
        .unwrap_or_else(|| DEFAULT_WINDOW_HEIGHT.max(default_h))
        .clamp(MIN_WINDOW_HEIGHT, max_h.max(MIN_WINDOW_HEIGHT));

    let max_x = monitor_pos
        .x
        .saturating_add(monitor_size.width as i32 - target_w as i32);
    let max_y = monitor_pos
        .y
        .saturating_add(monitor_size.height as i32 - target_h as i32);

    let fallback_x = monitor_pos
        .x
        .saturating_add((monitor_size.width as i32 - target_w as i32) / 2);
    let fallback_y = monitor_pos
        .y
        .saturating_add((monitor_size.height as i32 - target_h as i32) / 2);

    let target_x = if has_saved_position {
        settings
            .window_x
            .unwrap_or(fallback_x)
            .clamp(monitor_pos.x, max_x.max(monitor_pos.x))
    } else {
        fallback_x
    };
    let target_y = if has_saved_position {
        settings
            .window_y
            .unwrap_or(fallback_y)
            .clamp(monitor_pos.y, max_y.max(monitor_pos.y))
    } else {
        fallback_y
    };

    window
        .set_size(Size::Physical(PhysicalSize::new(target_w, target_h)))
        .map_err(|err| format!("设置窗口尺寸失败: {err}"))?;
    window
        .set_position(Position::Physical(PhysicalPosition::new(
            target_x, target_y,
        )))
        .map_err(|err| format!("设置窗口位置失败: {err}"))?;

    if !has_saved_size && !has_saved_position {
        let _ = window.center();
    }

    if settings.window_maximized.unwrap_or(false) {
        let _ = window.maximize();
    }

    Ok(())
}

fn persist_main_window_state(window: &tauri::Window) -> Result<(), String> {
    let mut settings = load_app_settings()?;
    let minimized = window
        .is_minimized()
        .map_err(|err| format!("读取窗口最小化状态失败: {err}"))?;
    if minimized {
        return Ok(());
    }
    let maximized = window
        .is_maximized()
        .map_err(|err| format!("读取窗口最大化状态失败: {err}"))?;

    settings.window_maximized = Some(maximized);
    if !maximized {
        let size = window
            .outer_size()
            .map_err(|err| format!("读取窗口尺寸失败: {err}"))?;
        let pos = window
            .outer_position()
            .map_err(|err| format!("读取窗口位置失败: {err}"))?;
        if pos.x <= -10_000 || pos.y <= -10_000 {
            return Ok(());
        }
        if size.width < MIN_WINDOW_WIDTH || size.height < MIN_WINDOW_HEIGHT {
            return Ok(());
        }
        settings.window_width = Some(size.width);
        settings.window_height = Some(size.height);
        settings.window_x = Some(pos.x);
        settings.window_y = Some(pos.y);
    }

    save_app_settings(&settings)
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

fn apply_model_settings_to_env(settings: ActiveRuntimeModelSettings) {
    // SAFETY: set_var affects process env; desktop runtime is single process and this is used as runtime config source.
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

fn build_answer_question(query: &str, lang: Option<&str>) -> String {
    match normalize_language(lang) {
        Some("zh-CN") => format!("{query}\n\n请仅使用中文回答。"),
        Some("en-US") => format!("{query}\n\nPlease answer in English only."),
        _ => query.to_string(),
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

fn normalize_top_k(top_k: Option<usize>) -> usize {
    match top_k {
        Some(value) if (1..=50).contains(&value) => value,
        _ => DEFAULT_RETRIEVE_TOP_K,
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

fn build_text_context(results: &[(DocumentChunk, f32)]) -> String {
    let mut parts = Vec::with_capacity(results.len());
    for (idx, (chunk, score)) in results.iter().enumerate() {
        parts.push(format!(
            "片段#{idx}\n来源: {}\n块序号: {}\n相似度: {:.4}\n内容:\n{}",
            chunk.file_path.display(),
            chunk.chunk_index,
            score,
            chunk.content,
            idx = idx + 1
        ));
    }
    parts.join("\n\n")
}

fn build_reference_excerpt(file_path: &std::path::Path, chunk_content: &str) -> String {
    const TARGET_EXCERPT_CHARS: usize = 1600;

    let Ok(raw) = std::fs::read_to_string(file_path) else {
        return chunk_content.to_string();
    };

    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let paragraphs: Vec<&str> = normalized
        .split("\n\n")
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();

    if paragraphs.is_empty() {
        return chunk_content.to_string();
    }

    let chunk_normalized = chunk_content.trim();
    let anchor = chunk_normalized
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && line.chars().count() >= 8)
        .unwrap_or(chunk_normalized);

    let paragraph_index = paragraphs
        .iter()
        .position(|paragraph| paragraph.contains(chunk_normalized))
        .or_else(|| {
            paragraphs
                .iter()
                .position(|paragraph| paragraph.contains(anchor))
        });

    let Some(index) = paragraph_index else {
        return chunk_content.to_string();
    };

    let mut start = index;
    let mut end = index + 1;
    let mut total_chars: usize = paragraphs[index].chars().count();

    while total_chars < TARGET_EXCERPT_CHARS && (start > 0 || end < paragraphs.len()) {
        let prev_len = if start > 0 {
            paragraphs[start - 1].chars().count()
        } else {
            0
        };
        let next_len = if end < paragraphs.len() {
            paragraphs[end].chars().count()
        } else {
            0
        };

        if next_len >= prev_len && end < paragraphs.len() {
            total_chars += next_len;
            end += 1;
            continue;
        }

        if start > 0 {
            start -= 1;
            total_chars += prev_len;
            continue;
        }

        if end < paragraphs.len() {
            total_chars += next_len;
            end += 1;
        }
    }

    paragraphs[start..end].join("\n\n")
}

fn format_references(results: &[(DocumentChunk, f32)]) -> String {
    let mut lines = Vec::with_capacity(results.len() * 4);
    for (idx, (chunk, score)) in results.iter().enumerate() {
        lines.push(format!("#{}  相似度: {:.4}", idx + 1, score));
        lines.push(format!("来源: {}", chunk.file_path.display()));
        lines.push(format!("块序号: {}", chunk.chunk_index));
        lines.push(build_reference_excerpt(&chunk.file_path, &chunk.content));
        lines.push(String::from(
            "------------------------------------------------------------",
        ));
    }
    lines.join("\n")
}
