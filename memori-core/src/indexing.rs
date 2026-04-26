use super::*;

const EMBEDDING_CHUNK_TIMEOUT_SECS: u64 = 120;

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
        timeout_secs = EMBEDDING_CHUNK_TIMEOUT_SECS,
        "indexing embedding started"
    );

    let mut embeddings = Vec::with_capacity(chunks.len());
    for (chunk_position, chunk) in chunks.iter().enumerate() {
        let chunk_number = chunk_position + 1;
        debug!(
            path = %event.path.display(),
            chunk_index = chunk.chunk_index,
            chunk_number = chunk_number,
            chunk_count = chunks.len(),
            char_len = chunk.content.chars().count(),
            "embedding chunk started"
        );

        match tokio::time::timeout(
            Duration::from_secs(EMBEDDING_CHUNK_TIMEOUT_SECS),
            state.embedding_client.embed_text(&chunk.content),
        )
        .await
        {
            Ok(Ok(embedding)) => {
                debug!(
                    path = %event.path.display(),
                    chunk_index = chunk.chunk_index,
                    chunk_number = chunk_number,
                    chunk_count = chunks.len(),
                    dim = embedding.len(),
                    "embedding chunk finished"
                );
                embeddings.push(embedding);
            }
            Ok(Err(err)) => {
                error!(
                    path = %event.path.display(),
                    chunk_index = chunk.chunk_index,
                    chunk_number = chunk_number,
                    chunk_count = chunks.len(),
                    error = %err,
                    "embedding chunk failed"
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
                    format!("embedding chunk timed out after {EMBEDDING_CHUNK_TIMEOUT_SECS}s");
                error!(
                    path = %event.path.display(),
                    chunk_index = chunk.chunk_index,
                    chunk_number = chunk_number,
                    chunk_count = chunks.len(),
                    timeout_secs = EMBEDDING_CHUNK_TIMEOUT_SECS,
                    "embedding chunk timed out"
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

pub(crate) async fn run_graph_worker(
    state: Arc<AppState>,
    mut notify_rx: mpsc::Receiver<()>,
) -> Result<(), EngineError> {
    info!("memori-core graph worker started");
    match state.vector_store.reset_running_graph_tasks().await {
        Ok(count) if count > 0 => {
            info!(
                count = count,
                "reset interrupted running graph tasks to pending"
            );
        }
        Ok(_) => {}
        Err(err) => warn!(error = %err, "failed to reset interrupted graph tasks"),
    }
    match state.vector_store.mark_orphan_graph_tasks_done().await {
        Ok(count) if count > 0 => {
            warn!(
                count = count,
                "discarded orphan graph tasks before graph extraction"
            );
        }
        Ok(_) => {}
        Err(err) => warn!(error = %err, "failed to discard orphan graph tasks"),
    }
    let mut channel_closed = false;

    loop {
        let runtime = state.indexing_runtime.read().await.clone();
        if runtime.paused
            || runtime.config.mode == IndexingMode::Manual
            || !is_within_schedule_window(&runtime.config)
        {
            sleep(Duration::from_millis(500)).await;
            if channel_closed && state.vector_store.count_graph_backlog().await.unwrap_or(0) == 0 {
                break;
            }
            continue;
        }

        // 一次收集一批任务，并行处理以减少 LLM 等待时间。
        // 注意：llama-server --parallel 参数决定并发槽位数，需与其保持一致。
        const GRAPH_BATCH_SIZE: usize = 2;
        let mut tasks = Vec::with_capacity(GRAPH_BATCH_SIZE);
        while tasks.len() < GRAPH_BATCH_SIZE {
            match state.vector_store.fetch_next_graph_task().await? {
                Some(task) => tasks.push(task),
                None => break,
            }
        }

        if tasks.is_empty() {
            if channel_closed {
                if state.vector_store.count_graph_backlog().await.unwrap_or(0) == 0 {
                    break;
                }
            } else {
                match notify_rx.recv().await {
                    Some(_) => {}
                    None => channel_closed = true,
                }
            }
            let cfg = state.indexing_runtime.read().await.config.clone();
            sleep(graph_worker_idle_delay(cfg.resource_budget)).await;
            continue;
        }

        {
            let mut runtime = state.indexing_runtime.write().await;
            runtime.phase = "graphing".to_string();
        }

        // 并行图谱抽取（附带原始索引确保结果对应正确）
        let mut live_tasks = Vec::with_capacity(tasks.len());
        for task in tasks {
            match state.vector_store.get_chunk_by_id(task.chunk_id).await {
                Ok(Some(_)) => live_tasks.push(task),
                Ok(None) => {
                    warn!(
                        chunk_id = task.chunk_id,
                        "graph task points to a deleted chunk; marking it done before LLM extraction"
                    );
                    state
                        .vector_store
                        .mark_graph_task_done(task.task_id)
                        .await?;
                }
                Err(err) => {
                    warn!(
                        chunk_id = task.chunk_id,
                        error = %err,
                        "failed to verify graph task chunk before extraction"
                    );
                    state
                        .vector_store
                        .mark_graph_task_failed(task.task_id, task.retry_count + 1)
                        .await?;
                }
            }
        }
        let tasks = live_tasks;
        if tasks.is_empty() {
            let mut runtime = state.indexing_runtime.write().await;
            runtime.phase = "idle".to_string();
            runtime.last_error = None;
            continue;
        }

        let mut set = tokio::task::JoinSet::new();
        for (idx, task) in tasks.iter().enumerate() {
            let content = task.content.clone();
            set.spawn(async move { (idx, extract_entities(&content).await) });
        }
        let mut results: Vec<Option<Result<GraphData, EngineError>>> =
            (0..tasks.len()).map(|_| None).collect();
        while let Some(result) = set.join_next().await {
            match result {
                Ok((idx, Ok(data))) => results[idx] = Some(Ok(data)),
                Ok((idx, Err(err))) => results[idx] = Some(Err(err)),
                Err(join_err) => {
                    warn!(error = %join_err, "图谱抽取任务异常终止");
                }
            }
        }

        // 顺序落盘（SQLite 单写更安全）
        let mut had_error = false;
        for (task, result) in tasks.into_iter().zip(results.into_iter()) {
            let graph_data = match result {
                Some(Ok(data)) => data,
                Some(Err(err)) => {
                    warn!(
                        chunk_id = task.chunk_id,
                        retry = task.retry_count + 1,
                        error = %err,
                        "图谱抽取失败，任务将重试"
                    );
                    state
                        .vector_store
                        .mark_graph_task_failed(task.task_id, task.retry_count + 1)
                        .await?;
                    had_error = true;
                    continue;
                }
                None => continue,
            };

            if let Err(err) = state
                .vector_store
                .insert_graph(task.chunk_id, graph_data.nodes, graph_data.edges)
                .await
            {
                // ChunkNotFound 意味着对应 chunk 已被重新索引删除，重试无意义，直接丢弃任务。
                if err.to_string().contains("chunk_id 不存在")
                    || err.to_string().contains("ChunkNotFound")
                {
                    warn!(
                        chunk_id = task.chunk_id,
                        "对应 chunk 已被删除，丢弃过期图谱任务"
                    );
                    state
                        .vector_store
                        .mark_graph_task_done(task.task_id)
                        .await?;
                } else {
                    warn!(
                        chunk_id = task.chunk_id,
                        retry = task.retry_count + 1,
                        error = %err,
                        "图谱落盘失败，任务将重试"
                    );
                    state
                        .vector_store
                        .mark_graph_task_failed(task.task_id, task.retry_count + 1)
                        .await?;
                    had_error = true;
                }
                continue;
            }

            state
                .vector_store
                .mark_graph_task_done(task.task_id)
                .await?;
        }

        let mut runtime = state.indexing_runtime.write().await;
        runtime.phase = "idle".to_string();
        if !had_error {
            runtime.last_error = None;
        }
    }

    info!("memori-core graph worker exiting");
    Ok(())
}

pub(crate) fn graph_worker_idle_delay(budget: ResourceBudget) -> Duration {
    match budget {
        ResourceBudget::Low => Duration::from_millis(650),
        ResourceBudget::Balanced => Duration::from_millis(260),
        ResourceBudget::Fast => Duration::from_millis(80),
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
        state.vector_store.purge_all_index_data().await?;

        let existing_files = collect_supported_text_files_recursively(root.to_path_buf()).await;
        info!(
            root = %root.display(),
            reason = reason,
            file_count = existing_files.len(),
            "开始执行全量重建"
        );

        for path in existing_files {
            let event = WatchEvent {
                kind: WatchEventKind::Modified,
                path,
                old_path: None,
                observed_at: SystemTime::now(),
            };
            process_file_event(state, &event, None, Some(root), true).await;

            let runtime = state.indexing_runtime.read().await.clone();
            if let Some(message) = runtime.last_error.filter(|msg| !msg.trim().is_empty()) {
                return Err(EngineError::IndexUnavailable {
                    reason: Some(format!("rebuild_file_failed:{message}")),
                });
            }
        }

        state.vector_store.finish_full_rebuild().await?;
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
            let failure_reason = format!("rebuild_failed:{err}");
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
            set_runtime_idle(state, Some(err.to_string())).await;
            Err(err)
        }
    }
}

pub(crate) async fn collect_supported_text_files_recursively(root: PathBuf) -> Vec<PathBuf> {
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
                    stack.push(path);
                }
                Ok(file_type) if file_type.is_file() => {
                    if is_supported_text_file(&path) {
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
    let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
        return false;
    };
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "md" | "txt" | "docx" | "pdf"
    )
}

/// Read document text. For binary formats (docx/pdf) delegates to memori-parser extraction.
async fn read_document_text(path: &std::path::Path) -> Result<String, std::io::Error> {
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
