use std::path::PathBuf;
use std::sync::Arc;

use memori_core::{AppState, DocumentChunk, MemoriEngine, VaultStats};
use tauri::{Manager, State};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

struct DesktopState {
    app_state: Arc<AppState>,
    engine: Arc<Mutex<MemoriEngine>>,
}

#[tauri::command]
async fn ask_vault(query: String, state: State<'_, DesktopState>) -> Result<String, String> {
    let query = query.trim().to_string();
    if query.is_empty() {
        return Ok("请输入一个非空问题。".to_string());
    }

    // 显式读取 app_state，确保其生命周期被 Tauri 状态托管。
    let _app_state_ref = Arc::clone(&state.app_state);

    let engine = Arc::clone(&state.engine);
    let engine_guard = engine.lock().await;

    let results = engine_guard
        .search(&query, 3)
        .await
        .map_err(|err| err.to_string())?;
    if results.is_empty() {
        return Ok("未检索到相关记忆。".to_string());
    }

    let text_context = build_text_context(&results);
    let graph_context = match engine_guard.get_graph_context_for_results(&results).await {
        Ok(context) => context,
        Err(err) => {
            warn!(error = %err, "图谱上下文构建失败，降级为纯文本上下文回答");
            String::new()
        }
    };

    let references = format_references(&results);
    match engine_guard
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
    let engine = Arc::clone(&state.engine);
    let engine_guard = engine.lock().await;
    engine_guard
        .get_vault_stats()
        .await
        .map_err(|err| err.to_string())
}

pub fn run() {
    let _ = tracing_subscriber::fmt()
        .with_target(false)
        .with_thread_ids(true)
        .with_level(true)
        .try_init();

    let watch_root = resolve_watch_root();
    let engine = match MemoriEngine::bootstrap(watch_root.clone()) {
        Ok(engine) => engine,
        Err(err) => {
            error!(error = %err, "初始化 MemoriEngine 失败");
            return;
        }
    };

    let app_state = engine.state();
    let shared_engine = Arc::new(Mutex::new(engine));
    let daemon_engine = Arc::clone(&shared_engine);

    tauri::Builder::default()
        .setup(move |app| {
            let daemon_watch_root = watch_root.clone();
            tauri::async_runtime::spawn(async move {
                let mut guard = daemon_engine.lock().await;
                match guard.start_daemon() {
                    Ok(()) => info!(
                        watch_root = %daemon_watch_root.display(),
                        "memori-desktop daemon started in setup hook"
                    ),
                    Err(err) => error!(error = %err, "memori-desktop daemon start failed"),
                }
            });

            app.manage(DesktopState {
                app_state: Arc::clone(&app_state),
                engine: Arc::clone(&shared_engine),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![ask_vault, get_vault_stats])
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
