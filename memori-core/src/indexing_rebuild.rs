use super::*;
use std::sync::Arc;
use tokio::sync::mpsc;

pub(crate) async fn run_full_rebuild(
    state: &Arc<AppState>,
    root: &std::path::Path,
    graph_notify_tx: Option<&mpsc::Sender<()>>,
    reason: &str,
) -> Result<(), EngineError> {
    let previous_paused = {
        let mut runtime = state.indexing_runtime.write().await;
        let paused = runtime.paused;
        runtime.paused = true;
        runtime.phase = "scanning".to_string();
        runtime.last_scan_at = Some(unix_now_secs());
        runtime.last_error = None;
        paused
    };

    let rebuild_result = async {
        state.vector_store.begin_full_rebuild(reason).await?;
        let is_resume = reason.starts_with("resume_") || reason.contains("resume");

        let filter = { state.indexing_runtime.read().await.filter_config.clone() };
        let existing_files = if is_resume {
            state.vector_store.list_retryable_file_index_paths().await?
        } else {
            collect_supported_text_files_recursively(
                root.to_path_buf(),
                filter.as_ref(),
                Some(root),
            )
            .await
        };
        info!(
            root = %root.display(),
            reason = reason,
            file_count = existing_files.len(),
            "开始执行全量重建"
        );

        let mut last_retryable_error: Option<String> = None;
        for path in &existing_files {
            let event = WatchEvent {
                kind: WatchEventKind::Modified,
                path: path.clone(),
                old_path: None,
                observed_at: SystemTime::now(),
            };
            process_file_event(state, &event, None, Some(root), true).await;

            let runtime = state.indexing_runtime.read().await.clone();
            if let Some(message) = runtime.last_error.filter(|msg| !msg.trim().is_empty()) {
                last_retryable_error = Some(message.clone());
                warn!(
                    path = %event.path.display(),
                    error = %message,
                    "index file failed; preserving progress and continuing rebuild"
                );
            }
        }

        let retryable_left = state.vector_store.list_retryable_file_index_paths().await?;
        if retryable_left.is_empty() {
            if !is_resume {
                let discovered: std::collections::HashSet<PathBuf> =
                    existing_files.iter().cloned().collect();
                let stale_paths = state.vector_store.list_active_catalog_file_paths().await?;
                for stale_path in stale_paths {
                    if !discovered.contains(&stale_path) {
                        remove_indexed_file(
                            state,
                            &stale_path,
                            "file was not discovered during rebuild; clearing stale index",
                        )
                        .await;
                    }
                }
            }
            state.vector_store.finish_full_rebuild().await?;
        } else {
            let reason =
                retryable_rebuild_reason(retryable_left.len(), last_retryable_error.as_deref());
            state
                .vector_store
                .mark_rebuild_required(reason.clone())
                .await?;
            return Err(EngineError::IndexUnavailable {
                reason: Some(reason),
            });
        }
        Ok::<(), EngineError>(())
    }
    .await;

    {
        let mut runtime = state.indexing_runtime.write().await;
        runtime.paused = previous_paused;
        runtime.last_scan_at = Some(unix_now_secs());
    }

    match rebuild_result {
        Ok(()) => {
            if !previous_paused && let Some(tx) = graph_notify_tx {
                let _ = tx.try_send(());
            }
            set_runtime_idle(state, None).await;
            Ok(())
        }
        Err(err) => {
            let retryable_reason = match &err {
                EngineError::IndexUnavailable {
                    reason: Some(reason),
                } if is_retryable_rebuild_reason(reason) => Some(reason.clone()),
                _ => None,
            };
            let failure_reason = retryable_reason
                .clone()
                .unwrap_or_else(|| format!("rebuild_failed:{err}"));
            if let Err(mark_err) = state
                .vector_store
                .mark_rebuild_required(failure_reason.clone())
                .await
            {
                warn!(
                    error = %mark_err,
                    "全量重建失败后写回 required 状态也失败"
                );
            }
            let last_error = retryable_reason
                .as_deref()
                .and_then(retryable_last_error)
                .unwrap_or_else(|| err.to_string());
            set_runtime_idle(state, Some(last_error)).await;
            Err(err)
        }
    }
}

pub(crate) async fn collect_supported_text_files_recursively(
    root: PathBuf,
    filter: Option<&crate::IndexFilterConfig>,
    watch_root: Option<&std::path::Path>,
) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![root];

    while let Some(dir) = stack.pop() {
        let mut read_dir = match tokio::fs::read_dir(&dir).await {
            Ok(reader) => reader,
            Err(err) => {
                warn!(
                    path = %dir.display(),
                    error = %err,
                    "递归扫描目录失败，已跳过该目录"
                );
                continue;
            }
        };

        loop {
            let next = match read_dir.next_entry().await {
                Ok(entry) => entry,
                Err(err) => {
                    warn!(
                        path = %dir.display(),
                        error = %err,
                        "读取目录项失败，已跳过剩余目录项"
                    );
                    break;
                }
            };

            let Some(entry) = next else {
                break;
            };

            let path = entry.path();
            match entry.file_type().await {
                Ok(file_type) if file_type.is_dir() => {
                    // 目录也应用排除规则，避免递归进入被排除的目录
                    if crate::filter::should_index_file(&path, filter, watch_root, None, None) {
                        stack.push(path);
                    } else {
                        debug!(path = %path.display(), "目录被筛选规则排除，跳过递归");
                    }
                }
                Ok(file_type) if file_type.is_file() => {
                    if is_supported_text_file(&path)
                        && crate::filter::should_index_file(&path, filter, watch_root, None, None)
                    {
                        files.push(path);
                    }
                }
                Ok(_) => {}
                Err(err) => {
                    warn!(
                        path = %path.display(),
                        error = %err,
                        "读取文件类型失败，已跳过该路径"
                    );
                }
            }
        }
    }

    files.sort();
    files
}

pub(crate) fn is_supported_text_file(path: &std::path::Path) -> bool {
    is_supported_content_file(path)
}

/// Read document text. For binary formats (docx/pdf) delegates to memori-parser extraction.
pub(crate) async fn read_document_text(path: &std::path::Path) -> Result<String, std::io::Error> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    if ext == "docx" || ext == "pdf" {
        // Binary formats: run extraction on blocking thread pool
        let path_buf = path.to_path_buf();
        match tokio::task::spawn_blocking(move || memori_parser::extract_document_text(&path_buf))
            .await
        {
            Ok(Some(text)) => Ok(text),
            Ok(None) => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to extract text from {}", path.display()),
            )),
            Err(join_err) => Err(std::io::Error::other(format!(
                "Extraction task panicked: {join_err}"
            ))),
        }
    } else {
        tokio::fs::read_to_string(path).await
    }
}
