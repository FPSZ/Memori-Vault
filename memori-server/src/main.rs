use std::collections::HashSet;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use memori_core::{DocumentChunk, MemoriEngine, VaultStats};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tower_http::cors::{Any, CorsLayer};
use tracing::{error, info, warn};

const DEFAULT_RETRIEVE_TOP_K: usize = 20;
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
}

#[derive(Debug, Clone, Serialize)]
struct AppSettingsDto {
    watch_root: String,
    language: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct AskRequest {
    query: String,
    lang: Option<String>,
    #[serde(default, alias = "topK")]
    top_k: Option<usize>,
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

    let results = engine
        .search(&query, top_k)
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

    if let Some(engine) = previous_engine
        && let Err(err) = engine.shutdown().await
    {
        warn!(error = %err, "关闭旧引擎失败，继续尝试重建");
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
