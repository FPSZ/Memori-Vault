use super::*;

impl MemoriEngine {
    /// 用现成 receiver 构造引擎（便于测试和外部注入）。
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

    /// 快速引导：创建事件通道 + 启动 memori-vault 监听端 + 初始化 SQLite 存储。
    pub fn bootstrap(root: impl Into<PathBuf>) -> Result<Self, EngineError> {
        let config = MemoriVaultConfig::new(root);
        Self::bootstrap_with_config(config)
    }

    /// 通过配置引导引擎。
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

    /// 读取共享状态句柄（供外部组件访问）。
    pub fn state(&self) -> Arc<AppState> {
        Arc::clone(&self.state)
    }

    /// 语义检索 API：
    /// 1) 先将 query 向量化；
    /// 2) 在向量存储中检索 top-k 相似块。
    pub async fn search(
        &self,
        query: &str,
        top_k: usize,
        scope_paths: Option<&[PathBuf]>,
    ) -> Result<Vec<(DocumentChunk, f32)>, EngineError> {
        if query.trim().is_empty() || top_k == 0 {
            return Ok(Vec::new());
        }

        ensure_search_ready(&self.state).await?;
        let query_embedding = self.embed_query_cached(query).await?;
        let results = self
            .state
            .vector_store
            .search_similar_scoped(query_embedding, top_k, scope_paths.unwrap_or(&[]))
            .await?;

        Ok(results)
    }

    pub async fn ask_structured(
        &self,
        query: &str,
        lang: Option<&str>,
        scope_paths: Option<&[PathBuf]>,
        final_answer_k: Option<usize>,
    ) -> Result<AskResponseStructured, EngineError> {
        let final_answer_k = final_answer_k
            .filter(|value| (1..=50).contains(value))
            .unwrap_or(DEFAULT_FINAL_ANSWER_K);
        let mut inspection = self
            .retrieve_structured(query, scope_paths, Some(final_answer_k))
            .await?;
        if inspection.status != AskStatus::Answered {
            return Ok(AskResponseStructured {
                status: inspection.status,
                answer: String::new(),
                question: inspection.question,
                scope_paths: inspection.scope_paths,
                citations: inspection.citations,
                evidence: inspection.evidence,
                metrics: inspection.metrics,
            });
        }

        let final_evidence = build_merged_evidence_from_items(&inspection.evidence);
        let answer_question = build_answer_question(&inspection.question, lang);
        let text_context = build_text_context_from_evidence(&final_evidence);
        let graph_seed = final_evidence
            .iter()
            .map(|item| (item.chunk.clone(), item.final_score as f32))
            .collect::<Vec<_>>();
        let graph_context = match self.get_graph_context_for_results(&graph_seed).await {
            Ok(context) => context,
            Err(err) => {
                warn!(error = %err, "图谱上下文构建失败，降级为纯文本上下文回答");
                String::new()
            }
        };

        let answer_started_at = Instant::now();
        let answer = match self
            .generate_answer(&answer_question, &text_context, &graph_context)
            .await
        {
            Ok(answer) => {
                inspection.metrics.answer_ms = elapsed_ms_u64(answer_started_at);
                answer
            }
            Err(err) => {
                warn!(error = %err, "答案生成失败，保留证据链返回");
                inspection.metrics.answer_ms = elapsed_ms_u64(answer_started_at);
                return Ok(AskResponseStructured {
                    status: AskStatus::ModelFailedWithEvidence,
                    answer: String::new(),
                    question: inspection.question,
                    scope_paths: inspection.scope_paths,
                    citations: inspection.citations,
                    evidence: inspection.evidence,
                    metrics: inspection.metrics,
                });
            }
        };

        if answer_indicates_insufficient_evidence(&answer) {
            return Ok(AskResponseStructured {
                status: AskStatus::InsufficientEvidence,
                answer: String::new(),
                question: inspection.question,
                scope_paths: inspection.scope_paths,
                citations: inspection.citations,
                evidence: inspection.evidence,
                metrics: inspection.metrics,
            });
        }

        Ok(AskResponseStructured {
            status: AskStatus::Answered,
            answer,
            question: inspection.question,
            scope_paths: inspection.scope_paths,
            citations: inspection.citations,
            evidence: inspection.evidence,
            metrics: inspection.metrics,
        })
    }

    pub async fn retrieve_structured(
        &self,
        query: &str,
        scope_paths: Option<&[PathBuf]>,
        final_answer_k: Option<usize>,
    ) -> Result<RetrievalInspection, EngineError> {
        if query.trim().is_empty() {
            return self
                .retrieve_structured_with_embedding(query, Vec::new(), scope_paths, final_answer_k)
                .await;
        }
        let query_embedding = self.embed_query_cached(query).await?;
        self.retrieve_structured_with_embedding(query, query_embedding, scope_paths, final_answer_k)
            .await
    }

    pub async fn retrieve_structured_with_embedding(
        &self,
        query: &str,
        query_embedding: Vec<f32>,
        scope_paths: Option<&[PathBuf]>,
        final_answer_k: Option<usize>,
    ) -> Result<RetrievalInspection, EngineError> {
        let question = query.trim().to_string();
        let final_answer_k = final_answer_k
            .filter(|value| (1..=50).contains(value))
            .unwrap_or(DEFAULT_FINAL_ANSWER_K);
        let normalized_scope_paths = scope_paths
            .unwrap_or(&[])
            .iter()
            .filter(|path| !path.as_os_str().is_empty())
            .cloned()
            .collect::<Vec<_>>();
        let serialized_scope_paths = normalized_scope_paths
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect::<Vec<_>>();

        if question.is_empty() {
            return Ok(RetrievalInspection {
                status: AskStatus::InsufficientEvidence,
                question,
                scope_paths: serialized_scope_paths,
                citations: Vec::new(),
                evidence: Vec::new(),
                metrics: RetrievalMetrics::default(),
            });
        }

        ensure_search_ready(&self.state).await?;
        let QueryPreparation {
            mut analysis,
            mut metrics,
        } = prepare_query_for_retrieval(&question);

        let doc_started_at = Instant::now();
        let candidate_docs = self
            .resolve_candidate_documents(&analysis, &normalized_scope_paths, &mut metrics)
            .await?;
        metrics.doc_recall_ms = elapsed_ms_u64(doc_started_at);
        metrics.doc_candidate_count = candidate_docs.len();

        if candidate_docs.is_empty() {
            if should_mark_missing_file_lookup_intent(&analysis) {
                analysis.query_intent = QueryIntent::MissingFileLookup;
                metrics
                    .query_flags
                    .retain(|flag| !flag.starts_with("intent:"));
                metrics
                    .query_flags
                    .push(format!("intent:{}", analysis.query_intent.as_str()));
            }
            return Ok(RetrievalInspection {
                status: AskStatus::InsufficientEvidence,
                question,
                scope_paths: serialized_scope_paths,
                citations: Vec::new(),
                evidence: Vec::new(),
                metrics,
            });
        }

        let candidate_scope_paths = candidate_docs
            .iter()
            .map(|doc| PathBuf::from(&doc.file_path))
            .collect::<Vec<_>>();
        let strict_lexical_started_at = Instant::now();
        let strict_lexical_matches = self
            .state
            .vector_store
            .search_chunks_fts_strict(
                &query_string_for_terms(&analysis.chunk_terms, &analysis.normalized_query),
                DEFAULT_CHUNK_CANDIDATE_K,
                &candidate_scope_paths,
            )
            .await?;
        metrics.chunk_strict_lexical_ms = elapsed_ms_u64(strict_lexical_started_at);

        let lexical_started_at = Instant::now();
        let lexical_matches = self
            .state
            .vector_store
            .search_chunks_fts(
                &query_string_for_terms(&analysis.chunk_terms, &analysis.normalized_query),
                DEFAULT_CHUNK_CANDIDATE_K,
                &candidate_scope_paths,
            )
            .await?;
        metrics.chunk_lexical_ms = elapsed_ms_u64(lexical_started_at);

        let dense_started_at = Instant::now();
        let dense_matches = self
            .state
            .vector_store
            .search_similar_scoped(
                query_embedding,
                DEFAULT_CHUNK_CANDIDATE_K,
                &candidate_scope_paths,
            )
            .await?;
        metrics.chunk_dense_ms = elapsed_ms_u64(dense_started_at);

        let merge_started_at = Instant::now();
        let merged = merge_chunk_evidence(
            &analysis,
            &candidate_docs,
            strict_lexical_matches,
            lexical_matches,
            dense_matches,
        );
        metrics.merge_ms = elapsed_ms_u64(merge_started_at);
        metrics.chunk_candidate_count = merged.len();

        if apply_gating_metrics(&mut metrics, &analysis, &merged) {
            return Ok(RetrievalInspection {
                status: AskStatus::InsufficientEvidence,
                question,
                scope_paths: serialized_scope_paths,
                citations: build_citations(&merged),
                evidence: build_evidence_items(&merged),
                metrics,
            });
        }

        let final_evidence = merged.into_iter().take(final_answer_k).collect::<Vec<_>>();
        metrics.final_evidence_count = final_evidence.len();
        let status = if final_evidence.len() < 2 {
            AskStatus::InsufficientEvidence
        } else {
            AskStatus::Answered
        };

        Ok(RetrievalInspection {
            status,
            question,
            scope_paths: serialized_scope_paths,
            citations: build_citations(&final_evidence),
            evidence: build_evidence_items(&final_evidence),
            metrics,
        })
    }

    async fn embed_query_cached(&self, query: &str) -> Result<Vec<f32>, EngineError> {
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

    async fn resolve_candidate_documents(
        &self,
        analysis: &QueryAnalysis,
        scope_paths: &[PathBuf],
        metrics: &mut RetrievalMetrics,
    ) -> Result<Vec<DocumentCandidate>, EngineError> {
        let doc_top_k = doc_top_k_for_query_family(analysis.query_family);
        let mut by_path = HashMap::<String, DocumentCandidate>::new();
        let file_scopes = scope_paths
            .iter()
            .filter(|path| is_supported_index_file(path))
            .cloned()
            .collect::<Vec<_>>();

        if !file_scopes.is_empty() {
            for (index, file_path) in file_scopes.iter().enumerate() {
                if let Some(record) = self
                    .state
                    .vector_store
                    .get_document_by_file_path(file_path)
                    .await?
                {
                    let is_code_document = is_code_document_path(&record.relative_path);
                    by_path.insert(
                        record.file_path.clone(),
                        DocumentCandidate {
                            file_path: record.file_path,
                            relative_path: record.relative_path,
                            file_name: record.file_name,
                            is_code_document,
                            document_reason: "scope".to_string(),
                            document_rank: index + 1,
                            document_raw_score: None,
                            exact_signal_score: None,
                            exact_path_score: None,
                            exact_symbol_score: None,
                            document_filename_score: None,
                            document_final_score: 10_000.0 - index as f64,
                            has_exact_signal: false,
                            has_exact_path_signal: false,
                            has_exact_symbol_signal: false,
                            has_docs_phrase_signal: false,
                            docs_phrase_quality: None,
                            has_filename_signal: false,
                            has_strict_lexical: true,
                            has_broad_lexical: true,
                        },
                    );
                }
            }
        }

        let file_only_scope = !scope_paths.is_empty() && file_scopes.len() == scope_paths.len();
        if file_only_scope && !by_path.is_empty() {
            let mut docs = by_path.into_values().collect::<Vec<_>>();
            docs.sort_by(|a, b| a.document_rank.cmp(&b.document_rank));
            return Ok(docs);
        }

        let doc_exact_started_at = Instant::now();
        let exact_docs = self
            .state
            .vector_store
            .search_documents_signal(&document_signal_query(analysis), doc_top_k, scope_paths)
            .await?;
        metrics.doc_exact_ms = elapsed_ms_u64(doc_exact_started_at);

        let phrase_docs = if analysis.docs_phrase_terms.is_empty() {
            Vec::new()
        } else {
            let doc_phrase_started_at = Instant::now();
            let docs = self
                .state
                .vector_store
                .search_documents_phrase_signal(&analysis.normalized_query, doc_top_k, scope_paths)
                .await?;
            metrics.doc_exact_ms += elapsed_ms_u64(doc_phrase_started_at);
            docs
        };

        let doc_strict_started_at = Instant::now();
        let strict_docs = self
            .state
            .vector_store
            .search_documents_fts_strict(
                &query_string_for_terms(&analysis.document_routing_terms, &analysis.lexical_query),
                doc_top_k,
                scope_paths,
            )
            .await?;
        metrics.doc_strict_lexical_ms = elapsed_ms_u64(doc_strict_started_at);

        let doc_lexical_started_at = Instant::now();
        let routed_docs = self
            .state
            .vector_store
            .search_documents_fts(
                &query_string_for_terms(&analysis.document_routing_terms, &analysis.lexical_query),
                doc_top_k,
                scope_paths,
            )
            .await?;
        metrics.doc_lexical_ms = elapsed_ms_u64(doc_lexical_started_at);

        let merge_started_at = Instant::now();
        let merged_docs =
            merge_document_candidates(analysis, exact_docs, phrase_docs, strict_docs, routed_docs);
        metrics.doc_merge_ms = elapsed_ms_u64(merge_started_at);

        for doc in merged_docs {
            by_path.entry(doc.file_path.clone()).or_insert(doc);
        }

        let mut docs = by_path.into_values().collect::<Vec<_>>();
        docs.sort_by(|a, b| {
            b.document_final_score
                .total_cmp(&a.document_final_score)
                .then_with(|| a.document_rank.cmp(&b.document_rank))
                .then_with(|| a.file_name.cmp(&b.file_name))
        });
        for (index, doc) in docs.iter_mut().enumerate() {
            doc.document_rank = index + 1;
        }
        Ok(docs)
    }

    /// 根据检索结果对应的 chunk_id，拉取 1-hop 图谱上下文。
    pub async fn get_graph_context_for_results(
        &self,
        results: &[(DocumentChunk, f32)],
    ) -> Result<String, EngineError> {
        if results.is_empty() {
            return Ok(String::new());
        }

        let mut chunk_ids = Vec::new();
        for (chunk, _score) in results {
            match self
                .state
                .vector_store
                .resolve_chunk_id(&chunk.file_path, chunk.chunk_index)
                .await?
            {
                Some(chunk_id) => chunk_ids.push(chunk_id),
                None => {
                    warn!(
                        path = %chunk.file_path.display(),
                        chunk_index = chunk.chunk_index,
                        "未能从检索结果反查 chunk_id，已跳过该条图谱上下文"
                    );
                }
            }
        }

        chunk_ids.sort_unstable();
        chunk_ids.dedup();

        let graph_context = self
            .state
            .vector_store
            .get_graph_context_for_chunks(&chunk_ids)
            .await?;

        Ok(graph_context)
    }

    /// 生成最终答案：融合向量文本上下文与图谱上下文。
    pub async fn generate_answer(
        &self,
        question: &str,
        text_context: &str,
        graph_context: &str,
    ) -> Result<String, EngineError> {
        generate_answer_with_context(question, text_context, graph_context).await
    }

    /// 返回当前 Vault 的核心规模统计。
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

        Ok(IndexingStatus {
            phase: runtime.phase.clone(),
            indexed_docs,
            indexed_chunks,
            graphed_chunks,
            graph_backlog,
            last_scan_at: runtime.last_scan_at,
            last_error: runtime.last_error.clone(),
            paused: runtime.paused,
            mode: runtime.config.mode,
            resource_budget: runtime.config.resource_budget,
            rebuild_state: metadata.rebuild_state.as_str().to_string(),
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

    pub async fn pause_indexing(&self) {
        let mut runtime = self.state.indexing_runtime.write().await;
        runtime.paused = true;
    }

    pub async fn resume_indexing(&self) {
        let mut runtime = self.state.indexing_runtime.write().await;
        runtime.paused = false;
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
            return run_full_rebuild(
                &self.state,
                &root,
                None,
                metadata
                    .rebuild_reason
                    .as_deref()
                    .unwrap_or("index_upgrade_required"),
            )
            .await;
        }

        match self.state.vector_store.load_from_db().await {
            Ok(loaded) => {
                info!(
                    loaded = loaded,
                    "已从本地数据库加载检索回归所需的历史向量缓存"
                );
            }
            Err(err) => {
                warn!(error = %err, "回归前加载本地数据库缓存失败，将继续执行一次全量扫描");
            }
        }

        {
            let mut runtime = self.state.indexing_runtime.write().await;
            runtime.phase = "scanning".to_string();
            runtime.last_scan_at = Some(unix_now_secs());
            runtime.last_error = None;
        }

        let existing_files = collect_supported_text_files_recursively(root.clone()).await;
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

    /// 启动异步守护任务，持续消费文件事件并触发解析、向量化与图谱提取流程。
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
                    error!(error = %err, "读取索引元数据失败，守护进程退出");
                    return Err(EngineError::Storage(err));
                }
            };

            if metadata.rebuild_state == RebuildState::Ready {
                match state.vector_store.load_from_db().await {
                    Ok(loaded) => {
                        info!(
                            loaded = loaded,
                            "已成功从本地数据库加载 [{}] 条历史向量记忆", loaded
                        );
                    }
                    Err(err) => {
                        error!(
                            error = %err,
                            "加载本地数据库历史记忆失败，将以空缓存继续运行"
                        );
                    }
                }
            } else {
                info!(
                    rebuild_state = metadata.rebuild_state.as_str(),
                    rebuild_reason = metadata.rebuild_reason.as_deref().unwrap_or(""),
                    "检测到索引版本不兼容，跳过旧缓存加载并准备全量重建"
                );
            }

            let runtime_cfg = { state.indexing_runtime.read().await.config.clone() };
            if let Some(root) = watch_root.clone()
                && runtime_cfg.mode != IndexingMode::Manual
                && is_within_schedule_window(&runtime_cfg)
            {
                if metadata.rebuild_state == RebuildState::Required
                    || metadata.rebuild_state == RebuildState::Rebuilding
                {
                    run_full_rebuild(
                        &state,
                        &root,
                        Some(&graph_notify_tx),
                        metadata
                            .rebuild_reason
                            .as_deref()
                            .unwrap_or("index_upgrade_required"),
                    )
                    .await?;
                } else {
                    {
                        let mut runtime = state.indexing_runtime.write().await;
                        runtime.phase = "scanning".to_string();
                        runtime.last_scan_at = Some(unix_now_secs());
                        runtime.last_error = None;
                    }

                    let existing_files =
                        collect_supported_text_files_recursively(root.clone()).await;
                    info!(
                        root = %root.display(),
                        file_count = existing_files.len(),
                        "启动时递归扫描完成，准备回灌子目录中的历史文档"
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

    /// 关闭引擎：
    /// 1) 优先停止 memori-vault（关闭发送端）；
    /// 2) 等待 daemon 消费完剩余事件后退出。
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
