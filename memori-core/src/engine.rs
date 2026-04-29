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
            let source_groups = build_source_groups(&inspection.citations, &inspection.evidence);
            return Ok(AskResponseStructured {
                status: inspection.status,
                answer: String::new(),
                question: inspection.question,
                scope_paths: inspection.scope_paths,
                citations: inspection.citations.clone(),
                evidence: inspection.evidence.clone(),
                metrics: inspection.metrics,
                answer_source_mix: inspection.answer_source_mix,
                memory_context: inspection.memory_context,
                source_groups,
                failure_class: inspection.failure_class,
                context_budget_report: inspection.context_budget_report,
            });
        }

        let final_evidence = build_merged_evidence_from_items(&inspection.evidence);
        let answer_question = build_answer_question(&inspection.question, lang);
        let (text_context, document_tokens) =
            build_text_context_from_evidence_with_budget(&final_evidence, 18_000);
        let (memory_context_text, memory_tokens) =
            build_memory_context_for_prompt(&inspection.memory_context, 3_000);
        let answer_question = if memory_context_text.is_empty() {
            answer_question
        } else {
            format!(
                "{answer_question}\n\nMEMORY CONTEXT (not document citation; use only as project/user context):\n{memory_context_text}"
            )
        };
        let graph_seed = final_evidence
            .iter()
            .map(|item| (item.chunk.clone(), item.final_score as f32))
            .collect::<Vec<_>>();
        let graph_context = match self.get_graph_context_for_results(&graph_seed).await {
            Ok(context) => context,
            Err(err) => {
                warn!(error = %err, "graph context build failed; falling back to text context");
                String::new()
            }
        };
        inspection.metrics.final_evidence_count = final_evidence.len();
        inspection.context_budget_report = ContextBudgetReport {
            token_budget: 16_000,
            used_by_documents: document_tokens,
            used_by_memory: memory_tokens,
            used_by_graph: estimate_tokens(&graph_context),
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
                warn!(error = %err, "answer generation failed; returning evidence");
                inspection.metrics.answer_ms = elapsed_ms_u64(answer_started_at);
                return Ok(AskResponseStructured {
                    status: AskStatus::ModelFailedWithEvidence,
                    answer: String::new(),
                    question: inspection.question,
                    scope_paths: inspection.scope_paths,
                    citations: inspection.citations.clone(),
                    evidence: inspection.evidence.clone(),
                    metrics: inspection.metrics,
                    answer_source_mix: inspection.answer_source_mix,
                    memory_context: inspection.memory_context,
                    source_groups: build_source_groups(&inspection.citations, &inspection.evidence),
                    failure_class: FailureClass::GenerationRefusal,
                    context_budget_report: inspection.context_budget_report,
                });
            }
        };

        if answer_indicates_insufficient_evidence(&answer) {
            return Ok(AskResponseStructured {
                status: AskStatus::InsufficientEvidence,
                answer: String::new(),
                question: inspection.question,
                scope_paths: inspection.scope_paths,
                citations: inspection.citations.clone(),
                evidence: inspection.evidence.clone(),
                metrics: inspection.metrics,
                answer_source_mix: AnswerSourceMix::Insufficient,
                memory_context: inspection.memory_context,
                source_groups: build_source_groups(&inspection.citations, &inspection.evidence),
                failure_class: FailureClass::GenerationRefusal,
                context_budget_report: inspection.context_budget_report,
            });
        }

        Ok(AskResponseStructured {
            status: AskStatus::Answered,
            answer,
            question: inspection.question,
            scope_paths: inspection.scope_paths,
            citations: inspection.citations.clone(),
            evidence: inspection.evidence.clone(),
            metrics: inspection.metrics,
            answer_source_mix: inspection.answer_source_mix,
            memory_context: inspection.memory_context,
            source_groups: build_source_groups(&inspection.citations, &inspection.evidence),
            failure_class: FailureClass::None,
            context_budget_report: inspection.context_budget_report,
        })
    }

    pub async fn retrieve_structured(
        &self,
        query: &str,
        scope_paths: Option<&[PathBuf]>,
        final_answer_k: Option<usize>,
    ) -> Result<RetrievalInspection, EngineError> {
        debug!(query = %query, scope_count = ?scope_paths.map(|s| s.len()), "retrieval started");
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
                answer_source_mix: AnswerSourceMix::Insufficient,
                memory_context: Vec::new(),
                source_groups: Vec::new(),
                failure_class: FailureClass::RecallMiss,
                context_budget_report: ContextBudgetReport::default(),
            });
        }

        ensure_search_ready(&self.state).await?;
        let QueryPreparation {
            mut analysis,
            mut metrics,
        } = prepare_query_for_retrieval(&question);
        debug!(intent = %analysis.query_intent.as_str(), family = %analysis.query_family.as_str(), flags = ?metrics.query_flags, "query analyzed");
        let memory_context = self.retrieve_memory_context(&analysis, 6).await?;

        let doc_started_at = Instant::now();
        let candidate_docs = self
            .resolve_candidate_documents(&analysis, &normalized_scope_paths, &mut metrics)
            .await?;
        metrics.doc_recall_ms = elapsed_ms_u64(doc_started_at);
        metrics.doc_candidate_count = candidate_docs.len();
        debug!(
            doc_count = candidate_docs.len(),
            doc_recall_ms = metrics.doc_recall_ms,
            "document recall done"
        );

        if candidate_docs.is_empty() {
            info!(reason = "no_candidate_documents", "证据不足，已拒答");
            if should_mark_missing_file_lookup_intent(&analysis) {
                analysis.query_intent = QueryIntent::MissingFileLookup;
                metrics
                    .query_flags
                    .retain(|flag| !flag.starts_with("intent:"));
                metrics
                    .query_flags
                    .push(format!("intent:{}", analysis.query_intent.as_str()));
            }
            let allow_memory_only = should_allow_memory_only_answer(&analysis, &memory_context);
            return Ok(RetrievalInspection {
                status: if allow_memory_only {
                    AskStatus::Answered
                } else {
                    AskStatus::InsufficientEvidence
                },
                question,
                scope_paths: serialized_scope_paths,
                citations: Vec::new(),
                evidence: Vec::new(),
                metrics,
                answer_source_mix: if allow_memory_only {
                    AnswerSourceMix::MemoryOnly
                } else {
                    AnswerSourceMix::Insufficient
                },
                memory_context,
                source_groups: Vec::new(),
                failure_class: if allow_memory_only {
                    FailureClass::None
                } else {
                    FailureClass::RecallMiss
                },
                context_budget_report: ContextBudgetReport::default(),
            });
        }

        let candidate_scope_paths = candidate_docs
            .iter()
            .map(|doc| PathBuf::from(&doc.file_path))
            .collect::<Vec<_>>();
        let chunk_query = query_string_for_terms(&analysis.chunk_terms, &analysis.normalized_query);
        let strict_lexical_started_at = Instant::now();
        let lexical_started_at = Instant::now();
        let dense_started_at = Instant::now();
        let strict_future = self.state.vector_store.search_chunks_fts_strict(
            &chunk_query,
            DEFAULT_CHUNK_CANDIDATE_K,
            &candidate_scope_paths,
        );
        let lexical_future = self.state.vector_store.search_chunks_fts(
            &chunk_query,
            DEFAULT_CHUNK_CANDIDATE_K,
            &candidate_scope_paths,
        );
        let dense_future = self.state.vector_store.search_similar_scoped(
            query_embedding,
            DEFAULT_CHUNK_CANDIDATE_K,
            &candidate_scope_paths,
        );
        let (strict_lexical_matches, lexical_matches, dense_matches) =
            tokio::try_join!(strict_future, lexical_future, dense_future)?;
        metrics.chunk_strict_lexical_ms = elapsed_ms_u64(strict_lexical_started_at);
        metrics.chunk_lexical_ms = elapsed_ms_u64(lexical_started_at);
        metrics.chunk_dense_ms = elapsed_ms_u64(dense_started_at);
        debug!(
            strict_count = strict_lexical_matches.len(),
            broad_count = lexical_matches.len(),
            dense_count = dense_matches.len(),
            strict_ms = metrics.chunk_strict_lexical_ms,
            broad_ms = metrics.chunk_lexical_ms,
            dense_ms = metrics.chunk_dense_ms,
            "chunk searches completed"
        );

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
        debug!(
            merged_count = merged.len(),
            ms = metrics.merge_ms,
            "evidence merge done"
        );

        if apply_gating_metrics(&mut metrics, &analysis, &merged) {
            info!(reason = %metrics.gating_decision_reason, "gating blocked answer as insufficient evidence");
            let citations = build_citations(&merged);
            let evidence_items = build_evidence_items(&merged);
            let source_groups = build_source_groups(&citations, &evidence_items);
            let allow_memory_only = should_allow_memory_only_answer(&analysis, &memory_context);
            return Ok(RetrievalInspection {
                status: if allow_memory_only {
                    AskStatus::Answered
                } else {
                    AskStatus::InsufficientEvidence
                },
                question,
                scope_paths: serialized_scope_paths,
                citations,
                evidence: evidence_items,
                metrics,
                answer_source_mix: if allow_memory_only {
                    AnswerSourceMix::MemoryOnly
                } else {
                    AnswerSourceMix::Insufficient
                },
                memory_context,
                source_groups,
                failure_class: if allow_memory_only {
                    FailureClass::None
                } else {
                    FailureClass::GatingFalseNegative
                },
                context_budget_report: ContextBudgetReport::default(),
            });
        }

        let final_evidence = merged.into_iter().take(final_answer_k).collect::<Vec<_>>();
        metrics.final_evidence_count = final_evidence.len();
        info!(
            final_count = final_evidence.len(),
            "retrieval completed; entering answer generation"
        );
        let status = if final_evidence.is_empty() {
            AskStatus::InsufficientEvidence
        } else {
            AskStatus::Answered
        };

        let citations = build_citations(&final_evidence);
        let evidence_items = build_evidence_items(&final_evidence);
        Ok(RetrievalInspection {
            status,
            question,
            scope_paths: serialized_scope_paths,
            source_groups: build_source_groups(&citations, &evidence_items),
            citations,
            evidence: evidence_items,
            metrics,
            answer_source_mix: if status == AskStatus::Answered {
                if memory_context.is_empty() {
                    AnswerSourceMix::DocumentOnly
                } else {
                    AnswerSourceMix::DocumentPlusMemory
                }
            } else {
                AnswerSourceMix::Insufficient
            },
            memory_context,
            failure_class: if status == AskStatus::Answered {
                FailureClass::None
            } else {
                FailureClass::RecallMiss
            },
            context_budget_report: ContextBudgetReport::default(),
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

    async fn retrieve_memory_context(
        &self,
        analysis: &QueryAnalysis,
        limit: usize,
    ) -> Result<Vec<MemoryEvidence>, EngineError> {
        if should_skip_memory_context(analysis) {
            return Ok(Vec::new());
        }

        let records = self
            .state
            .vector_store
            .search_memories(MemorySearchOptions {
                query: analysis.normalized_query.clone(),
                scope: None,
                layer: None,
                limit,
            })
            .await?;

        Ok(records
            .into_iter()
            .filter(|record| {
                matches!(record.status, MemoryStatus::Active | MemoryStatus::Pending)
                    && !matches!(record.source_type, MemorySourceType::DocumentChunk)
            })
            .map(memory_record_to_evidence)
            .collect())
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
            docs.sort_by_key(|a| a.document_rank);
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
                        "could not resolve chunk_id from retrieval result; skipping graph context item"
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

    pub async fn generate_answer(
        &self,
        question: &str,
        text_context: &str,
        graph_context: &str,
    ) -> Result<String, EngineError> {
        generate_answer_with_context(question, text_context, graph_context).await
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
        let progress_percent = match runtime.phase.as_str() {
            "scanning" => ((indexed_docs as f64 / total_docs.max(1) as f64) * 33.0) as u32,
            "embedding" => {
                33 + ((indexed_chunks as f64 / total_chunks.max(1) as f64) * 33.0) as u32
            }
            "graphing" => 66 + ((graphed_chunks as f64 / total_chunks.max(1) as f64) * 34.0) as u32,
            _ if metadata.rebuild_state == memori_storage::RebuildState::Ready => 100,
            _ => 0,
        }
        .min(100);

        Ok(IndexingStatus {
            phase: runtime.phase.clone(),
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

fn memory_record_to_evidence(record: MemoryRecord) -> MemoryEvidence {
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

fn should_skip_memory_context(analysis: &QueryAnalysis) -> bool {
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
