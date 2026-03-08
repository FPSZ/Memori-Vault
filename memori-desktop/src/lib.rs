use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use memori_core::{DocumentChunk, MemoriEngine, VaultStats};
use serde::{Deserialize, Serialize};
use tauri::{Manager, State};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

const DEFAULT_RETRIEVE_TOP_K: usize = 20;
const SETTINGS_APP_DIR_NAME: &str = "Memori-Vault";
const SETTINGS_FILE_NAME: &str = "settings.json";

struct DesktopState {
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
struct SettingsSearchCandidate {
    key: String,
    text: String,
}

#[tauri::command]
async fn ask_vault(
    query: String,
    lang: Option<String>,
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

    let results = engine
        .search(&query, DEFAULT_RETRIEVE_TOP_K)
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
async fn get_app_settings() -> Result<AppSettingsDto, String> {
    let settings = load_app_settings()?;
    let watch_root = resolve_watch_root_from_settings(&settings)?;
    Ok(AppSettingsDto {
        watch_root: watch_root.to_string_lossy().to_string(),
        language: settings.language,
    })
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

    Ok(AppSettingsDto {
        watch_root: canonical.to_string_lossy().to_string(),
        language: settings.language,
    })
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

    if let (Some(start), Some(end)) = (answer.find('['), answer.rfind(']')) {
        if start < end {
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

    let shared_engine = Arc::new(Mutex::new(None));
    let daemon_engine = Arc::clone(&shared_engine);
    let init_error = Arc::new(Mutex::new(None));
    let init_error_worker = Arc::clone(&init_error);

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
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

            app.manage(DesktopState {
                engine: Arc::clone(&shared_engine),
                init_error: Arc::clone(&init_error),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ask_vault,
            get_vault_stats,
            get_app_settings,
            set_watch_root,
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
        if let Err(err) = engine.shutdown().await {
            warn!(error = %err, "关闭旧引擎失败，继续尝试重建");
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
    let Some(lang) = lang else {
        return None;
    };
    let lower = lang.trim().to_ascii_lowercase();
    if lower.starts_with("zh") {
        Some("zh-CN")
    } else if lower.starts_with("en") {
        Some("en-US")
    } else {
        None
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
