use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use memori_core::{DocumentChunk, MemoriEngine, VaultStats};
use tauri::{Manager, State};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

struct DesktopState {
    engine: Arc<Mutex<Option<MemoriEngine>>>,
    init_error: Arc<Mutex<Option<String>>>,
}

#[tauri::command]
async fn ask_vault(query: String, state: State<'_, DesktopState>) -> Result<String, String> {
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
        .search(&query, 3)
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

    let references = format_references(&results);
    match engine
        .generate_answer(&query, &text_context, &graph_context)
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
        let status = Command::new("explorer")
            .arg(format!("/select,{}", target.display()))
            .status()
            .map_err(|err| format!("打开文件位置失败: {err}"))?;
        if !status.success() {
            return Err("打开文件位置失败: explorer 返回非零状态".to_string());
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

pub fn run() {
    let _ = tracing_subscriber::fmt()
        .with_target(false)
        .with_thread_ids(true)
        .with_level(true)
        .try_init();

    let watch_root = resolve_watch_root();
    let shared_engine = Arc::new(Mutex::new(None));
    let daemon_engine = Arc::clone(&shared_engine);
    let init_error = Arc::new(Mutex::new(None));
    let init_error_worker = Arc::clone(&init_error);

    tauri::Builder::default()
        .setup(move |app| {
            let daemon_watch_root = watch_root.clone();
            tauri::async_runtime::spawn(async move {
                match MemoriEngine::bootstrap(daemon_watch_root.clone()) {
                    Ok(mut engine) => match engine.start_daemon() {
                        Ok(()) => {
                            let mut guard = daemon_engine.lock().await;
                            *guard = Some(engine);
                            info!(
                                watch_root = %daemon_watch_root.display(),
                                "memori-desktop daemon started in setup hook"
                            );
                        }
                        Err(err) => {
                            let mut init_err_guard = init_error_worker.lock().await;
                            *init_err_guard = Some(err.to_string());
                            error!(error = %err, "memori-desktop daemon start failed");
                        }
                    },
                    Err(err) => {
                        let mut init_err_guard = init_error_worker.lock().await;
                        *init_err_guard = Some(err.to_string());
                        error!(error = %err, "memori-desktop engine bootstrap failed");
                    }
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
            open_source_location
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|err| {
            error!(error = %err, "tauri runtime exited with error");
        });
}

fn resolve_watch_root() -> PathBuf {
    if let Ok(path) = std::env::var("MEMORI_WATCH_ROOT") {
        return PathBuf::from(path);
    }

    match std::env::current_dir() {
        Ok(path) => path,
        Err(_) => PathBuf::from("."),
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
