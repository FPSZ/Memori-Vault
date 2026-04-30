use super::*;

impl MemoriEngine {
    pub fn new(state: Arc<AppState>, event_rx: mpsc::Receiver<WatchEvent>) -> Self {
        Self {
            state,
            event_rx: Some(event_rx),
            daemon_task: None,
            graph_worker_task: None,
            memori_vault_handle: None,
            watch_root: None,
            graph_notify_tx: None,
        }
    }

    pub fn bootstrap(root: impl Into<PathBuf>) -> Result<Self, EngineError> {
        let config = MemoriVaultConfig::new(root);
        Self::bootstrap_with_config(config)
    }

    pub fn bootstrap_with_config(config: MemoriVaultConfig) -> Result<Self, EngineError> {
        let watch_root = config.root.clone();
        let (event_tx, event_rx) = create_event_channel();
        let memori_vault_handle = spawn_memori_vault(config, event_tx)?;
        let db_path = resolve_db_path()?;

        let state = Arc::new(AppState::new(db_path)?);
        let mut engine = Self::new(state, event_rx);
        engine.memori_vault_handle = Some(memori_vault_handle);
        engine.watch_root = Some(watch_root);
        Ok(engine)
    }

    pub fn state(&self) -> Arc<AppState> {
        Arc::clone(&self.state)
    }

    pub(crate) async fn embed_query_cached(&self, query: &str) -> Result<Vec<f32>, EngineError> {
        let query_key = query.trim().to_string();
        let now = unix_now_secs();
        let cached = {
            let cache_guard = self.state.query_embedding_cache.read().await;
            cache_guard.get(&query_key).and_then(|item| {
                if now - item.cached_at <= QUERY_EMBEDDING_CACHE_TTL_SECS {
                    Some(item.embedding.clone())
                } else {
                    None
                }
            })
        };

        if let Some(embedding) = cached {
            return Ok(embedding);
        }

        let embedding = self.state.embedding_client.embed_text(query).await?;
        let mut cache_guard = self.state.query_embedding_cache.write().await;
        if cache_guard.len() >= QUERY_EMBEDDING_CACHE_SIZE {
            let stale_key = cache_guard
                .iter()
                .min_by_key(|(_, item)| item.cached_at)
                .map(|(key, _)| key.clone());
            if let Some(stale_key) = stale_key {
                cache_guard.remove(&stale_key);
            }
        }
        cache_guard.insert(
            query_key,
            EmbeddingCacheItem {
                embedding: embedding.clone(),
                cached_at: now,
            },
        );
        Ok(embedding)
    }

    pub async fn get_vault_stats(&self) -> Result<VaultStats, EngineError> {
        let document_count = self.state.vector_store.count_documents().await?;
        let chunk_count = self.state.vector_store.count_chunks().await?;
        let graph_node_count = self.state.vector_store.count_nodes().await?;

        Ok(VaultStats {
            document_count,
            chunk_count,
            graph_node_count,
        })
    }

    pub async fn get_indexing_status(&self) -> Result<IndexingStatus, EngineError> {
        let runtime = self.state.indexing_runtime.read().await;
        let metadata = self.state.vector_store.read_index_metadata().await?;
        let indexed_docs = self.state.vector_store.count_documents().await?;
        let indexed_chunks = self.state.vector_store.count_chunks().await?;
        let graphed_chunks = self.state.vector_store.count_graphed_chunks().await?;
        let graph_backlog = self.state.vector_store.count_graph_backlog().await?;
        let total_docs = self
            .state
            .vector_store
            .count_catalog_entries()
            .await
            .unwrap_or(0);
        let total_chunks = indexed_chunks.max(1);
        let model_connection_blocked = {
            let text = format!(
                "{} {}",
                runtime.last_error.as_deref().unwrap_or_default(),
                metadata.rebuild_reason.as_deref().unwrap_or_default()
            )
            .to_ascii_lowercase();
            [
                "embedding request failed",
                "connection refused",
                "actively refused",
                "failed to connect",
                "error sending request",
                "timed out",
                "timeout",
                "tcp connect error",
                "connectex",
            ]
            .iter()
            .any(|pattern| text.contains(pattern))
        };
        let phase = if model_connection_blocked {
            "idle".to_string()
        } else {
            runtime.phase.clone()
        };
        let rebuild_state = if model_connection_blocked {
            memori_storage::RebuildState::Required
        } else {
            metadata.rebuild_state
        };
        let progress_percent = if model_connection_blocked {
            0
        } else {
            match runtime.phase.as_str() {
                "scanning" => ((indexed_docs as f64 / total_docs.max(1) as f64) * 33.0) as u32,
                "embedding" => {
                    33 + ((indexed_chunks as f64 / total_chunks.max(1) as f64) * 33.0) as u32
                }
                "graphing" => {
                    let graph_total = graphed_chunks + graph_backlog;
                    let done = graph_total.saturating_sub(graph_backlog);
                    66 + ((done as f64 / graph_total.max(1) as f64) * 34.0) as u32
                }
                _ if metadata.rebuild_state == memori_storage::RebuildState::Ready => 100,
                _ => 0,
            }
            .min(100)
        };

        Ok(IndexingStatus {
            phase,
            indexed_docs,
            indexed_chunks,
            graphed_chunks,
            graph_backlog,
            total_docs,
            total_chunks,
            progress_percent,
            last_scan_at: runtime.last_scan_at,
            last_error: runtime.last_error.clone(),
            paused: runtime.paused,
            mode: runtime.config.mode,
            resource_budget: runtime.config.resource_budget,
            rebuild_state: rebuild_state.as_str().to_string(),
            rebuild_reason: metadata.rebuild_reason,
            index_format_version: metadata.index_format_version,
            parser_format_version: metadata.parser_format_version,
        })
    }

    pub async fn get_runtime_retrieval_baseline(
        &self,
    ) -> Result<RuntimeRetrievalBaseline, EngineError> {
        let metadata = self.state.vector_store.read_index_metadata().await?;
        let indexed_document_count = self.state.vector_store.count_documents().await?;
        let indexed_chunk_count = self.state.vector_store.count_chunks().await?;
        let embedding_dim = self
            .state
            .vector_store
            .embedding_dimension()
            .await?
            .unwrap_or_default();

        Ok(RuntimeRetrievalBaseline {
            watch_root: self
                .watch_root
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
            resolved_db_path: self.state.db_path.to_string_lossy().to_string(),
            embedding_model_key: self.state.embedding_client.model_name().to_string(),
            embedding_dim,
            indexed_document_count,
            indexed_chunk_count,
            rebuild_state: metadata.rebuild_state.as_str().to_string(),
        })
    }

    pub async fn set_indexing_config(&self, config: IndexingConfig) {
        let mut runtime = self.state.indexing_runtime.write().await;
        runtime.config = config;
    }

    pub async fn set_index_filter_config(&self, filter: Option<crate::IndexFilterConfig>) {
        let mut runtime = self.state.indexing_runtime.write().await;
        runtime.filter_config = filter;
    }

    pub async fn pause_indexing(&self) {
        let mut runtime = self.state.indexing_runtime.write().await;
        runtime.paused = true;
    }

    pub async fn resume_indexing(&self) {
        {
            let mut runtime = self.state.indexing_runtime.write().await;
            runtime.paused = false;
        }

        if let Some(root) = self.watch_root.clone() {
            let state = Arc::clone(&self.state);
            let graph_notify_tx = self.graph_notify_tx.clone();
            tokio::spawn(async move {
                if let Err(err) =
                    run_full_rebuild(&state, &root, graph_notify_tx.as_ref(), "resume_indexing")
                        .await
                {
                    warn!(error = %err, "resume indexing did not finish all retryable files");
                }
            });
        }
    }

    pub async fn trigger_reindex(&self) -> Result<(), EngineError> {
        let Some(root) = self.watch_root.clone() else {
            return Ok(());
        };
        run_full_rebuild(
            &self.state,
            &root,
            self.graph_notify_tx.as_ref(),
            "manual_reindex",
        )
        .await
    }

    pub async fn prepare_retrieval_index(&self) -> Result<(), EngineError> {
        let Some(root) = self.watch_root.clone() else {
            return Ok(());
        };

        let metadata = self.state.vector_store.read_index_metadata().await?;
        if metadata.rebuild_state == RebuildState::Required
            || metadata.rebuild_state == RebuildState::Rebuilding
        {
            let rebuild_reason = metadata
                .rebuild_reason
                .as_deref()
                .unwrap_or("index_upgrade_required");
            let recover_existing_progress = rebuild_reason.contains("retryable_files_remaining")
                || rebuild_reason.starts_with("rebuild_failed:Index unavailable")
                || rebuild_reason.starts_with("rebuild_failed:index is not ready")
                || rebuild_reason.starts_with("rebuild_failed:索引不可用");
            let reason = if recover_existing_progress {
                format!("resume_prepare:{rebuild_reason}")
            } else {
                rebuild_reason.to_string()
            };
            return run_full_rebuild(&self.state, &root, None, &reason).await;
        }

        match self.state.vector_store.load_from_db().await {
            Ok(loaded) => {
                info!(
                    loaded = loaded,
                    "loaded historical vectors from local database before regression"
                );
            }
            Err(err) => {
                warn!(error = %err, "failed to load local database cache before regression; continuing with full scan");
            }
        }

        {
            let mut runtime = self.state.indexing_runtime.write().await;
            runtime.phase = "scanning".to_string();
            runtime.last_scan_at = Some(unix_now_secs());
            runtime.last_error = None;
        }

        let filter = {
            self.state
                .indexing_runtime
                .read()
                .await
                .filter_config
                .clone()
        };
        let existing_files =
            collect_supported_text_files_recursively(root.clone(), filter.as_ref(), Some(&root))
                .await;
        for path in existing_files {
            let event = WatchEvent {
                kind: WatchEventKind::Modified,
                path,
                old_path: None,
                observed_at: SystemTime::now(),
            };
            process_file_event(&self.state, &event, None, Some(&root), false).await;
        }

        set_runtime_idle(&self.state, None).await;
        let mut runtime = self.state.indexing_runtime.write().await;
        runtime.last_scan_at = Some(unix_now_secs());
        Ok(())
    }

    pub fn start_daemon(&mut self) -> Result<(), EngineError> {
        if self.daemon_task.is_some() {
            return Err(EngineError::DaemonAlreadyStarted);
        }

        let (graph_notify_tx, graph_notify_rx) = mpsc::channel::<()>(32);
        self.graph_notify_tx = Some(graph_notify_tx.clone());

        let mut event_rx = self
            .event_rx
            .take()
            .ok_or(EngineError::EventChannelUnavailable)?;
        let state = Arc::clone(&self.state);
        let watch_root = self.watch_root.clone();
        let graph_worker_state = Arc::clone(&self.state);

        let graph_task =
            tokio::spawn(
                async move { run_graph_worker(graph_worker_state, graph_notify_rx).await },
            );

        let task = tokio::spawn(async move {
            info!("memori-core daemon started");

            let metadata = match state.vector_store.read_index_metadata().await {
                Ok(metadata) => metadata,
                Err(err) => {
                    error!(error = %err, "failed to read index metadata; daemon exiting");
                    return Err(EngineError::Storage(err));
                }
            };

            if metadata.rebuild_state == RebuildState::Ready {
                match state.vector_store.load_from_db().await {
                    Ok(loaded) => {
                        info!(
                            loaded = loaded,
                            "loaded historical vectors from local database"
                        );
                    }
                    Err(err) => {
                        error!(
                            error = %err,
                            "failed to load historical vectors from local database; continuing with empty cache"
                        );
                    }
                }
            } else {
                match state.vector_store.load_from_db().await {
                    Ok(loaded) => {
                        info!(
                            loaded = loaded,
                            rebuild_state = metadata.rebuild_state.as_str(),
                            rebuild_reason = metadata.rebuild_reason.as_deref().unwrap_or(""),
                            "loaded preserved vectors before index recovery"
                        );
                    }
                    Err(err) => {
                        warn!(
                            error = %err,
                            rebuild_state = metadata.rebuild_state.as_str(),
                            "failed to load preserved vectors before index recovery"
                        );
                    }
                }
            }

            let runtime_cfg = { state.indexing_runtime.read().await.config.clone() };
            if let Some(root) = watch_root.clone()
                && runtime_cfg.mode != IndexingMode::Manual
                && is_within_schedule_window(&runtime_cfg)
            {
                if metadata.rebuild_state == RebuildState::Required
                    || metadata.rebuild_state == RebuildState::Rebuilding
                {
                    let rebuild_reason = metadata
                        .rebuild_reason
                        .as_deref()
                        .unwrap_or("index_upgrade_required");
                    let recover_existing_progress = rebuild_reason
                        .contains("retryable_files_remaining")
                        || rebuild_reason.starts_with("rebuild_failed:Index unavailable")
                        || rebuild_reason.starts_with("rebuild_failed:index is not ready")
                        || rebuild_reason.starts_with("rebuild_failed:索引不可用");
                    let reason = if recover_existing_progress {
                        format!("resume_startup:{rebuild_reason}")
                    } else {
                        rebuild_reason.to_string()
                    };
                    run_full_rebuild(&state, &root, Some(&graph_notify_tx), &reason).await?;
                }

                if metadata.rebuild_state == RebuildState::Ready {
                    {
                        let mut runtime = state.indexing_runtime.write().await;
                        runtime.phase = "scanning".to_string();
                        runtime.last_scan_at = Some(unix_now_secs());
                        runtime.last_error = None;
                    }

                    let filter = { state.indexing_runtime.read().await.filter_config.clone() };
                    let existing_files = collect_supported_text_files_recursively(
                        root.clone(),
                        filter.as_ref(),
                        Some(&root),
                    )
                    .await;
                    info!(
                        root = %root.display(),
                        file_count = existing_files.len(),
                        "startup recursive scan completed"
                    );

                    for path in existing_files {
                        let event = WatchEvent {
                            kind: WatchEventKind::Modified,
                            path,
                            old_path: None,
                            observed_at: SystemTime::now(),
                        };
                        process_file_event(
                            &state,
                            &event,
                            Some(&graph_notify_tx),
                            watch_root.as_deref(),
                            false,
                        )
                        .await;
                    }

                    let mut runtime = state.indexing_runtime.write().await;
                    runtime.phase = "idle".to_string();
                    runtime.last_scan_at = Some(unix_now_secs());
                }
            }

            while let Some(event) = event_rx.recv().await {
                let (paused, cfg) = {
                    let runtime = state.indexing_runtime.read().await;
                    (runtime.paused, runtime.config.clone())
                };
                if paused || cfg.mode == IndexingMode::Manual || !is_within_schedule_window(&cfg) {
                    continue;
                }

                match event.kind {
                    WatchEventKind::Created
                    | WatchEventKind::Modified
                    | WatchEventKind::Renamed
                    | WatchEventKind::Removed => {
                        process_file_event(
                            &state,
                            &event,
                            Some(&graph_notify_tx),
                            watch_root.as_deref(),
                            false,
                        )
                        .await;
                    }
                }
            }

            info!("memori-core event channel closed, daemon exiting");
            Ok(())
        });

        self.daemon_task = Some(task);
        self.graph_worker_task = Some(graph_task);
        Ok(())
    }

    pub async fn shutdown(mut self) -> Result<(), EngineError> {
        self.graph_notify_tx.take();

        if let Some(memori_vault_handle) = self.memori_vault_handle.take() {
            memori_vault_handle.join().await?;
        }

        if let Some(daemon_task) = self.daemon_task.take() {
            daemon_task.await??;
        }
        if let Some(graph_worker_task) = self.graph_worker_task.take() {
            graph_worker_task.await??;
        }

        Ok(())
    }
}

pub(crate) fn answer_indicates_insufficient_evidence(answer: &str) -> bool {
    let trimmed = answer.trim();
    if trimmed.is_empty() {
        return true;
    }

    let lower = trimmed.to_ascii_lowercase();
    [
        "当前上下文不足",
        "上下文不足",
        "证据不足",
        "insufficient context",
        "not enough context",
        "insufficient evidence",
        "not enough evidence",
        "lack sufficient context",
        "lack sufficient evidence",
    ]
    .iter()
    .any(|marker| trimmed.contains(marker) || lower.contains(marker))
}

pub(crate) fn memory_record_to_evidence(record: MemoryRecord) -> MemoryEvidence {
    MemoryEvidence {
        id: record.id,
        layer: record.layer,
        scope: record.scope,
        memory_type: record.memory_type,
        title: record.title,
        content: record.content,
        source_type: record.source_type,
        source_ref: record.source_ref,
        confidence: record.confidence,
        status: record.status,
    }
}

pub(crate) fn should_skip_memory_context(analysis: &QueryAnalysis) -> bool {
    matches!(
        analysis.query_intent,
        QueryIntent::ExternalFact | QueryIntent::SecretRequest | QueryIntent::MissingFileLookup
    )
}

pub(crate) fn should_allow_memory_only_answer(
    analysis: &QueryAnalysis,
    memory_context: &[MemoryEvidence],
) -> bool {
    !memory_context.is_empty()
        && !should_skip_memory_context(analysis)
        && is_memory_intent_query(&analysis.normalized_query)
}

fn is_memory_intent_query(query: &str) -> bool {
    let lower = query.to_ascii_lowercase();
    let ascii_markers = [
        "memory",
        "remember",
        "preference",
        "prefer",
        "previous",
        "earlier",
        "project decision",
        "what did i say",
        "what have i said",
        "my setting",
        "my context",
    ];
    if ascii_markers.iter().any(|marker| lower.contains(marker)) {
        return true;
    }

    let cjk_markers = [
        "记忆",
        "偏好",
        "喜好",
        "之前",
        "刚才",
        "上次",
        "我说过",
        "项目决策",
        "项目上下文",
        "会话摘要",
    ];
    cjk_markers.iter().any(|marker| query.contains(marker))
}

pub(crate) fn build_memory_context_for_prompt(
    memory_context: &[MemoryEvidence],
    max_chars: usize,
) -> (String, usize) {
    if memory_context.is_empty() || max_chars == 0 {
        return (String::new(), 0);
    }

    let mut parts = Vec::new();
    let mut used_chars = 0usize;
    for memory in memory_context {
        if used_chars >= max_chars {
            break;
        }
        let remaining = max_chars.saturating_sub(used_chars);
        let mut content = memory.content.clone();
        if content.chars().count() > remaining.saturating_sub(256) {
            content = content
                .chars()
                .take(remaining.saturating_sub(256).max(160))
                .collect::<String>();
            content.push_str("\n...[truncated by memory context budget]");
        }
        let part = format!(
            "Memory #{id}\nlayer: {layer:?}\nscope: {scope:?}\ntype: {memory_type}\nsource_type: {source_type:?}\nsource_ref: {source_ref}\nconfidence: {confidence:.2}\ntitle: {title}\ncontent:\n{content}",
            id = memory.id,
            layer = memory.layer,
            scope = memory.scope,
            memory_type = memory.memory_type,
            source_type = memory.source_type,
            source_ref = memory.source_ref,
            confidence = memory.confidence,
            title = memory.title,
            content = content,
        );
        used_chars += part.chars().count();
        parts.push(part);
    }

    let context = parts.join("\n\n");
    let tokens = estimate_tokens(&context);
    (context, tokens)
}
