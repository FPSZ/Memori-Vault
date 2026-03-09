use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use memori_core::{
    DEFAULT_CHAT_MODEL, DEFAULT_GRAPH_MODEL, DEFAULT_MODEL_ENDPOINT_OLLAMA, DEFAULT_MODEL_PROVIDER,
    DEFAULT_OLLAMA_EMBED_MODEL, DocumentChunk, MEMORI_CHAT_MODEL_ENV, MEMORI_EMBED_MODEL_ENV,
    MEMORI_GRAPH_MODEL_ENV, MEMORI_MODEL_API_KEY_ENV, MEMORI_MODEL_ENDPOINT_ENV,
    MEMORI_MODEL_PROVIDER_ENV, MemoriEngine, ModelProvider, VaultStats,
};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio::time::{Duration, timeout};
use tower_http::cors::{Any, CorsLayer};
use tracing::{error, info, warn};

const DEFAULT_RETRIEVE_TOP_K: usize = 20;
const ENGINE_SHUTDOWN_TIMEOUT_SECS: u64 = 8;
const PROVIDER_HTTP_TIMEOUT_SECS: u64 = 15;
const SETTINGS_APP_DIR_NAME: &str = "Memori-Vault";
const SETTINGS_FILE_NAME: &str = "settings.json";

#[derive(Clone)]
struct ServerState {
    engine: Arc<Mutex<Option<MemoriEngine>>>,
    init_error: Arc<Mutex<Option<String>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct AppSettings {
    watch_root: Option<String>,
    language: Option<String>,
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
struct AskRequest {
    query: String,
    lang: Option<String>,
    #[serde(default, alias = "topK")]
    top_k: Option<usize>,
    #[serde(default, alias = "scopePaths")]
    scope_paths: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
struct AskResponse {
    answer: String,
}

#[derive(Debug, Clone, Deserialize)]
struct SetWatchRootRequest {
    path: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ListProviderModelsRequest {
    provider: String,
    endpoint: String,
    api_key: Option<String>,
    models_root: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ProbeProviderRequest {
    provider: String,
    endpoint: String,
    api_key: Option<String>,
    models_root: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct PullModelRequest {
    model: String,
    provider: String,
    endpoint: String,
    api_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct SetLocalModelsRootRequest {
    path: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ScanLocalModelFilesRequest {
    root: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct SettingsSearchCandidate {
    key: String,
    text: String,
}

#[derive(Debug, Clone, Deserialize)]
struct RankSettingsRequest {
    query: String,
    candidates: Vec<SettingsSearchCandidate>,
    lang: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct RankSettingsResponse {
    keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct HealthResponse {
    ok: bool,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResponse {
                error: self.message,
            }),
        )
            .into_response()
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
    let model_settings = resolve_model_settings(&settings);
    apply_model_settings_to_env(resolve_active_runtime_settings(&model_settings));

    let engine = Arc::new(Mutex::new(None));
    let init_error = Arc::new(Mutex::new(None));
    if let Err(err) =
        replace_engine(&engine, &init_error, watch_root.clone(), "server_bootstrap").await
    {
        error!(error = %err, "memori-server bootstrap failed");
    }

    let app_state = ServerState { engine, init_error };
    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/stats", get(get_vault_stats_handler))
        .route("/api/ask", post(ask_handler))
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

async fn ask_handler(
    State(state): State<ServerState>,
    Json(payload): Json<AskRequest>,
) -> Result<Json<AskResponse>, ApiError> {
    let query = payload.query.trim().to_string();
    if query.is_empty() {
        return Ok(Json(AskResponse {
            answer: "请输入一个非空问题。".to_string(),
        }));
    }

    let init_error_message = state.init_error.lock().await.clone();
    let engine_guard = state.engine.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
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

    let results = engine
        .search(&query, top_k, scope_refs)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

    if results.is_empty() {
        return Ok(Json(AskResponse {
            answer: "未检索到相关记忆。".to_string(),
        }));
    }

    let text_context = build_text_context(&results);
    let graph_context = match engine.get_graph_context_for_results(&results).await {
        Ok(context) => context,
        Err(err) => {
            warn!(error = %err, "图谱上下文构建失败，降级为纯文本上下文回答");
            String::new()
        }
    };

    let answer_question = build_answer_question(&query, payload.lang.as_deref());
    let references = format_references(&results);
    let answer = match engine
        .generate_answer(&answer_question, &text_context, &graph_context)
        .await
    {
        Ok(answer) => format!("{answer}\n\n---\n参考来源：\n{references}"),
        Err(err) => {
            warn!(error = %err, "答案合成失败，降级返回向量检索结果");
            format!("本地大模型答案生成失败，以下是检索到的相关片段：\n\n{references}")
        }
    };

    Ok(Json(AskResponse { answer }))
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

async fn get_app_settings_handler() -> Result<Json<AppSettingsDto>, ApiError> {
    let settings = load_app_settings().map_err(ApiError::internal)?;
    let watch_root = resolve_watch_root_from_settings(&settings).map_err(ApiError::internal)?;
    Ok(Json(AppSettingsDto {
        watch_root: watch_root.to_string_lossy().to_string(),
        language: settings.language,
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

async fn validate_model_setup_handler() -> Result<Json<ModelAvailabilityDto>, ApiError> {
    let settings = load_app_settings().map_err(ApiError::internal)?;
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
    Json(payload): Json<ListProviderModelsRequest>,
) -> Result<Json<ProviderModelsDto>, ApiError> {
    let provider = ModelProvider::from_str(&payload.provider);
    let endpoint = normalize_endpoint(provider, &payload.endpoint);
    let api_key = normalize_optional_text(payload.api_key);
    let models_root = normalize_optional_text(payload.models_root);
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
    Json(payload): Json<ProbeProviderRequest>,
) -> Result<Json<ModelAvailabilityDto>, ApiError> {
    let provider = ModelProvider::from_str(&payload.provider);
    let endpoint = normalize_endpoint(provider, &payload.endpoint);
    let api_key = normalize_optional_text(payload.api_key);
    let models_root = normalize_optional_text(payload.models_root);
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
    Json(payload): Json<PullModelRequest>,
) -> Result<Json<ModelAvailabilityDto>, ApiError> {
    let model = payload.model.trim().to_string();
    if model.is_empty() {
        return Err(ApiError::bad_request("模型名不能为空"));
    }
    let provider = ModelProvider::from_str(&payload.provider);
    if provider != ModelProvider::OllamaLocal {
        return Err(ApiError::bad_request("仅本地 Ollama 模式支持拉取模型"));
    }
    let endpoint = normalize_endpoint(provider, &payload.endpoint);
    let api_key = normalize_optional_text(payload.api_key);
    pull_ollama_model(&endpoint, &model, api_key.as_deref())
        .await
        .map_err(ApiError::internal)?;
    validate_model_setup_handler().await
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

    Ok(Json(AppSettingsDto {
        watch_root: canonical.to_string_lossy().to_string(),
        language: settings.language,
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

    let mut new_engine =
        MemoriEngine::bootstrap(watch_root.clone()).map_err(|err| err.to_string())?;
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
    let active_provider = ModelProvider::from_str(&fallback_provider);
    let env_provider = std::env::var(MEMORI_MODEL_PROVIDER_ENV)
        .ok()
        .map(|value| ModelProvider::from_str(&value))
        .unwrap_or(active_provider);

    let local_endpoint = settings
        .local_endpoint
        .clone()
        .or_else(|| {
            if ModelProvider::from_str(
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
            if ModelProvider::from_str(
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
            if ModelProvider::from_str(
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
            if ModelProvider::from_str(
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
            if ModelProvider::from_str(
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
            if ModelProvider::from_str(
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
            if ModelProvider::from_str(
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
            if ModelProvider::from_str(
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
            if ModelProvider::from_str(
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
    let active_provider = ModelProvider::from_str(&payload.active_provider);
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
    let active_provider = ModelProvider::from_str(&settings.active_provider);
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

fn format_references(results: &[(DocumentChunk, f32)]) -> String {
    let mut lines = Vec::with_capacity(results.len() * 4);
    for (idx, (chunk, score)) in results.iter().enumerate() {
        lines.push(format!("#{}  相似度: {:.4}", idx + 1, score));
        lines.push(format!("来源: {}", chunk.file_path.display()));
        lines.push(format!("块序号: {}", chunk.chunk_index));
        lines.push(chunk.content.clone());
        lines.push(String::from(
            "------------------------------------------------------------",
        ));
    }
    lines.join("\n")
}
