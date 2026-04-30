use super::*;

const EMBEDDING_CHUNK_TIMEOUT_SECS: u64 = 120;
const EMBEDDING_BATCH_SIZE: usize = 16;

pub(crate) fn retryable_rebuild_reason(retryable_count: usize, last_error: Option<&str>) -> String {
    let clean_error = last_error
        .map(str::trim)
        .filter(|message| !message.is_empty())
        .map(|message| message.replace(['\r', '\n'], " "));

    match clean_error {
        Some(message) => {
            format!("retryable_files_remaining:{retryable_count}; last_error:{message}")
        }
        None => format!("retryable_files_remaining:{retryable_count}"),
    }
}

pub(crate) fn is_retryable_rebuild_reason(reason: &str) -> bool {
    reason.contains("retryable_files_remaining")
}

pub(crate) fn retryable_last_error(reason: &str) -> Option<String> {
    reason
        .split_once("last_error:")
        .map(|(_, message)| message.trim().to_string())
        .filter(|message| !message.is_empty())
}

pub(crate) async fn process_file_event(
    state: &Arc<AppState>,
    event: &WatchEvent,
    graph_notify_tx: Option<&mpsc::Sender<()>>,
    watch_root: Option<&std::path::Path>,
    allow_rebuild_write: bool,
) {
    if !allow_rebuild_write {
        match state.vector_store.read_index_metadata().await {
            Ok(metadata) if metadata.rebuild_state != RebuildState::Ready => {
                debug!(
                    path = %event.path.display(),
                    rebuild_state = metadata.rebuild_state.as_str(),
                    "索引当前不处于 ready 状态，已跳过文件事件写入"
                );
                return;
            }
            Ok(_) => {}
            Err(err) => {
                warn!(
                    path = %event.path.display(),
                    error = %err,
                    "读取索引元数据失败，已跳过文件事件"
                );
                return;
            }
        }
    }

    if matches!(event.kind, WatchEventKind::Removed) {
        remove_indexed_file(state, &event.path, "文件已删除，清理旧索引").await;
        return;
    }

    if matches!(event.kind, WatchEventKind::Renamed)
        && let Some(old_path) = event.old_path.as_ref()
        && old_path != &event.path
    {
        remove_indexed_file(state, old_path, "文件已重命名，清理旧路径索引").await;
    }

    if !is_supported_index_file(&event.path) {
        debug!(path = %event.path.display(), kind = ?event.kind, "目标路径不是受支持文本文件，跳过重建索引");
        set_runtime_idle(state, None).await;
        return;
    }

    {
        let mut runtime = state.indexing_runtime.write().await;
        runtime.phase = "scanning".to_string();
        runtime.last_scan_at = Some(unix_now_secs());
    }

    let metadata = match tokio::fs::metadata(&event.path).await {
        Ok(meta) => meta,
        Err(err) => {
            warn!(
                path = %event.path.display(),
                error = %err,
                "读取文件元数据失败，跳过本次索引"
            );
            let mut runtime = state.indexing_runtime.write().await;
            runtime.last_error = Some(err.to_string());
            runtime.phase = "idle".to_string();
            return;
        }
    };
    let file_size = i64::try_from(metadata.len()).unwrap_or(i64::MAX);
    let mtime_secs = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .unwrap_or(0);

    // 应用筛选规则
    {
        let runtime = state.indexing_runtime.read().await;
        if let Some(ref filter) = runtime.filter_config
            && !crate::filter::should_index_file(
                &event.path,
                Some(filter),
                watch_root,
                Some(metadata.len()),
                Some(mtime_secs),
            )
        {
            debug!(path = %event.path.display(), "文件被筛选规则排除，跳过索引");
            set_runtime_idle(state, None).await;
            return;
        }
    }

    if let Err(err) = state
        .vector_store
        .upsert_catalog_entry(&event.path, watch_root, file_size, mtime_secs)
        .await
    {
        warn!(
            path = %event.path.display(),
            error = %err,
            "更新文件目录索引失败，已跳过本次事件"
        );
        set_runtime_idle(state, Some(err.to_string())).await;
        return;
    }
    let previous_state = state
        .vector_store
        .get_file_index_state(&event.path)
        .await
        .ok()
        .flatten();
    let previous_index_is_ready = previous_state
        .as_ref()
        .is_some_and(|prev| prev.index_status == "ready");
    if let Some(prev) = previous_state.as_ref()
        && prev.file_size == file_size
        && prev.mtime_secs == mtime_secs
        && previous_index_is_ready
    {
        debug!(path = %event.path.display(), "文件元数据未变化，跳过重建索引");
        set_runtime_idle(state, None).await;
        return;
    }

    let raw_text = match read_document_text(&event.path).await {
        Ok(text) => text,
        Err(err) => {
            warn!(
                path = %event.path.display(),
                error = %err,
                "文件读取失败（可能被占用），已跳过"
            );
            let mut runtime = state.indexing_runtime.write().await;
            runtime.last_error = Some(err.to_string());
            runtime.phase = "idle".to_string();
            return;
        }
    };
    let file_hash = hash_text(&raw_text);
    if let Some(prev) = previous_state.as_ref()
        && prev.content_hash == file_hash
        && previous_index_is_ready
    {
        debug!(path = %event.path.display(), "文件内容哈希未变化，跳过重建索引");
        if let Err(err) = state
            .vector_store
            .upsert_file_index_state(&event.path, file_size, mtime_secs, &file_hash)
            .await
        {
            warn!(
                path = %event.path.display(),
                error = %err,
                "刷新文件索引元数据失败"
            );
        }
        set_runtime_idle(state, None).await;
        return;
    }

    if let Err(err) = state
        .vector_store
        .mark_file_index_pending(&event.path, file_size, mtime_secs, &file_hash)
        .await
    {
        warn!(
            path = %event.path.display(),
            error = %err,
            "写入文件待索引状态失败，继续执行本次索引"
        );
    }

    let chunks = match parse_and_chunk(&event.path, &raw_text) {
        Ok(chunks) => {
            info!(
                path = %event.path.display(),
                chunk_count = chunks.len(),
                "文件 [{}] 已成功解析，共生成 [{}] 个文本块。",
                event.path.display(),
                chunks.len()
            );
            chunks
        }
        Err(err) => {
            warn!(
                path = %event.path.display(),
                error = %err,
                "解析失败，已跳过本次事件"
            );
            let _ = state
                .vector_store
                .mark_file_index_failed(
                    &event.path,
                    file_size,
                    mtime_secs,
                    &file_hash,
                    &err.to_string(),
                )
                .await;
            let mut runtime = state.indexing_runtime.write().await;
            runtime.last_error = Some(err.to_string());
            runtime.phase = "idle".to_string();
            return;
        }
    };

    if chunks.is_empty() {
        debug!(path = %event.path.display(), "解析结果为空，清理旧索引并保留 catalog 记录");
        if let Err(err) = state.vector_store.purge_file_path(&event.path).await {
            warn!(
                path = %event.path.display(),
                error = %err,
                "清理空文档旧索引失败"
            );
        }
        let _ = state
            .vector_store
            .upsert_catalog_entry(&event.path, watch_root, file_size, mtime_secs)
            .await;
        if let Err(err) = state
            .vector_store
            .upsert_file_index_state(&event.path, file_size, mtime_secs, &file_hash)
            .await
        {
            warn!(
                path = %event.path.display(),
                error = %err,
                "写入空文档索引状态失败"
            );
        }
        set_runtime_idle(state, None).await;
        return;
    }

    {
        let mut runtime = state.indexing_runtime.write().await;
        runtime.phase = "embedding".to_string();
    }

    info!(
        path = %event.path.display(),
        chunk_count = chunks.len(),
        model = state.embedding_client.model_name(),
        "indexing embedding started"
    );

    let mut embeddings = Vec::with_capacity(chunks.len());
    for (batch_index, batch) in chunks.chunks(EMBEDDING_BATCH_SIZE).enumerate() {
        let batch_start = batch_index * EMBEDDING_BATCH_SIZE;
        let batch_end = batch_start + batch.len();
        debug!(
            path = %event.path.display(),
            batch_start = batch_start + 1,
            batch_end = batch_end,
            chunk_count = chunks.len(),
            batch_size = batch.len(),
            "embedding batch started"
        );
        let prompts = batch
            .iter()
            .map(|chunk| chunk.content.clone())
            .collect::<Vec<_>>();

        match tokio::time::timeout(
            Duration::from_secs(EMBEDDING_CHUNK_TIMEOUT_SECS),
            state.embedding_client.embed_batch(&prompts),
        )
        .await
        {
            Ok(Ok(mut batch_embeddings)) => {
                debug!(
                    path = %event.path.display(),
                    batch_start = batch_start + 1,
                    batch_end = batch_end,
                    batch_size = batch_embeddings.len(),
                    dim = batch_embeddings.first().map(|item| item.len()).unwrap_or_default(),
                    "embedding batch finished"
                );
                embeddings.append(&mut batch_embeddings);
            }
            Ok(Err(err)) => {
                error!(
                    path = %event.path.display(),
                    batch_start = batch_start + 1,
                    batch_end = batch_end,
                    chunk_count = chunks.len(),
                    error = %err,
                    "embedding batch failed"
                );
                let _ = state
                    .vector_store
                    .mark_file_index_failed(
                        &event.path,
                        file_size,
                        mtime_secs,
                        &file_hash,
                        &err.to_string(),
                    )
                    .await;
                let mut runtime = state.indexing_runtime.write().await;
                runtime.last_error = Some(err.to_string());
                runtime.phase = "idle".to_string();
                return;
            }
            Err(_) => {
                let err =
                    format!("embedding batch timed out after {EMBEDDING_CHUNK_TIMEOUT_SECS}s");
                error!(
                    path = %event.path.display(),
                    batch_start = batch_start + 1,
                    batch_end = batch_end,
                    chunk_count = chunks.len(),
                    "embedding batch timed out"
                );
                let _ = state
                    .vector_store
                    .mark_file_index_failed(&event.path, file_size, mtime_secs, &file_hash, &err)
                    .await;
                let mut runtime = state.indexing_runtime.write().await;
                runtime.last_error = Some(err);
                runtime.phase = "idle".to_string();
                return;
            }
        }
    }

    info!(
        path = %event.path.display(),
        chunk_count = chunks.len(),
        "indexing embedding finished"
    );

    if let Err(err) = state
        .vector_store
        .replace_document_index(
            &event.path,
            watch_root,
            mtime_secs,
            &file_hash,
            chunks.clone(),
            embeddings,
        )
        .await
    {
        error!(
            path = %event.path.display(),
            error = %err,
            "向量落盘失败，本次事件已跳过但守护进程继续运行"
        );
        let _ = state
            .vector_store
            .mark_file_index_failed(
                &event.path,
                file_size,
                mtime_secs,
                &file_hash,
                &err.to_string(),
            )
            .await;
        let mut runtime = state.indexing_runtime.write().await;
        runtime.last_error = Some(err.to_string());
        runtime.phase = "idle".to_string();
        return;
    }
    if let Err(err) = state
        .vector_store
        .upsert_file_index_state(&event.path, file_size, mtime_secs, &file_hash)
        .await
    {
        warn!(
            path = %event.path.display(),
            error = %err,
            "failed to refresh real file metadata after indexing"
        );
    }

    for chunk in chunks {
        let chunk_id = match state
            .vector_store
            .resolve_chunk_id(&chunk.file_path, chunk.chunk_index)
            .await
        {
            Ok(Some(id)) => id,
            Ok(None) => continue,
            Err(err) => {
                warn!(
                    path = %chunk.file_path.display(),
                    chunk_index = chunk.chunk_index,
                    error = %err,
                    "无法解析 chunk_id，跳过图谱任务入队"
                );
                continue;
            }
        };
        let chunk_hash = hash_text(&chunk.content);
        if let Err(err) = state
            .vector_store
            .enqueue_graph_task(chunk_id, &chunk_hash, &chunk.content)
            .await
        {
            warn!(
                path = %chunk.file_path.display(),
                chunk_index = chunk.chunk_index,
                error = %err,
                "图谱任务入队失败，后续可重试"
            );
        }
    }

    if let Some(tx) = graph_notify_tx {
        let _ = tx.try_send(());
    }

    set_runtime_idle(state, None).await;
}

pub(crate) async fn remove_indexed_file(
    state: &Arc<AppState>,
    file_path: &std::path::Path,
    reason: &str,
) {
    {
        let mut runtime = state.indexing_runtime.write().await;
        runtime.phase = "scanning".to_string();
        runtime.last_scan_at = Some(unix_now_secs());
    }

    let purge_result = if is_likely_directory_path(file_path) {
        state.vector_store.purge_directory_path(file_path).await
    } else {
        state.vector_store.purge_file_path(file_path).await
    };

    match purge_result {
        Ok(true) => {
            info!(path = %file_path.display(), reason = reason, "文件索引清理完成");
            set_runtime_idle(state, None).await;
        }
        Ok(false) => {
            debug!(path = %file_path.display(), reason = reason, "文件不存在可清理索引，跳过");
            set_runtime_idle(state, None).await;
        }
        Err(err) => {
            warn!(
                path = %file_path.display(),
                reason = reason,
                error = %err,
                "清理旧文件索引失败"
            );
            set_runtime_idle(state, Some(err.to_string())).await;
        }
    }
}

pub(crate) fn is_within_schedule_window(config: &IndexingConfig) -> bool {
    if config.mode != IndexingMode::Scheduled {
        return true;
    }
    let Some(window) = config.schedule_window.as_ref() else {
        return true;
    };

    let Some(start_minutes) = parse_hhmm_to_minutes(&window.start) else {
        return true;
    };
    let Some(end_minutes) = parse_hhmm_to_minutes(&window.end) else {
        return true;
    };

    let now = unix_now_secs();
    let day_secs = 24 * 60 * 60;
    let minute_now = ((now.rem_euclid(day_secs)) / 60) as i32;

    if start_minutes <= end_minutes {
        minute_now >= start_minutes && minute_now <= end_minutes
    } else {
        minute_now >= start_minutes || minute_now <= end_minutes
    }
}

pub(crate) fn parse_hhmm_to_minutes(text: &str) -> Option<i32> {
    let mut parts = text.trim().split(':');
    let hour = parts.next()?.parse::<i32>().ok()?;
    let minute = parts.next()?.parse::<i32>().ok()?;
    if parts.next().is_some() || !(0..=23).contains(&hour) || !(0..=59).contains(&minute) {
        return None;
    }
    Some(hour * 60 + minute)
}

pub(crate) fn hash_text(text: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

pub(crate) fn unix_now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .unwrap_or(0)
}
