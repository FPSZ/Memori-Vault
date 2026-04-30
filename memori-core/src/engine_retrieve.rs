use super::*;
use crate::engine::{memory_record_to_evidence, should_skip_memory_context};

impl MemoriEngine {

    pub(crate) async fn retrieve_evidence_for_analysis(
        &self,
        analysis: &QueryAnalysis,
        query_embedding: Vec<f32>,
        normalized_scope_paths: &[PathBuf],
        metrics: &mut RetrievalMetrics,
    ) -> Result<EvidenceRetrievalResult, EngineError> {
        let doc_started_at = Instant::now();
        let candidate_docs = self
            .resolve_candidate_documents(analysis, normalized_scope_paths, metrics)
            .await?;
        metrics.doc_recall_ms += elapsed_ms_u64(doc_started_at);
        metrics.doc_candidate_count = metrics.doc_candidate_count.max(candidate_docs.len());
        debug!(
            doc_count = candidate_docs.len(),
            doc_recall_ms = metrics.doc_recall_ms,
            "document recall done"
        );

        if candidate_docs.is_empty() {
            return Ok(EvidenceRetrievalResult {
                candidate_docs,
                evidence: Vec::new(),
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
        metrics.chunk_strict_lexical_ms += elapsed_ms_u64(strict_lexical_started_at);
        metrics.chunk_lexical_ms += elapsed_ms_u64(lexical_started_at);
        metrics.chunk_dense_ms += elapsed_ms_u64(dense_started_at);
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
        let evidence = merge_chunk_evidence(
            analysis,
            &candidate_docs,
            strict_lexical_matches,
            lexical_matches,
            dense_matches,
        );
        metrics.merge_ms += elapsed_ms_u64(merge_started_at);
        metrics.chunk_candidate_count = metrics.chunk_candidate_count.max(evidence.len());
        debug!(
            merged_count = evidence.len(),
            ms = metrics.merge_ms,
            "evidence merge done"
        );

        Ok(EvidenceRetrievalResult {
            candidate_docs,
            evidence,
        })
    }

    pub(crate) async fn retrieve_compound_evidence(
        &self,
        plan: &CompoundQueryPlan,
        _root_analysis: &QueryAnalysis,
        root_evidence: &[MergedEvidence],
        normalized_scope_paths: &[PathBuf],
        final_answer_k: usize,
        should_embed_parts: bool,
        metrics: &mut RetrievalMetrics,
    ) -> Result<CompoundEvidenceResult, EngineError> {
        let mut selected = Vec::<MergedEvidence>::new();
        let mut matched_parts = 0usize;

        for part in &plan.parts {
            let part_analysis = analyze_query(&part.query);
            if matches!(
                part_analysis.query_intent,
                QueryIntent::ExternalFact
                    | QueryIntent::SecretRequest
                    | QueryIntent::MissingFileLookup
            ) {
                continue;
            }
            let part_embedding = if should_embed_parts {
                self.embed_query_cached(&part.query).await?
            } else {
                Vec::new()
            };
            let mut part_metrics = RetrievalMetrics::default();
            let retrieval = self
                .retrieve_evidence_for_analysis(
                    &part_analysis,
                    part_embedding,
                    normalized_scope_paths,
                    &mut part_metrics,
                )
                .await?;
            accumulate_compound_metrics(metrics, &part_metrics);

            let decision = evaluate_gating_decision(&part_analysis, &retrieval.evidence);
            if decision.refuse
                && !compound_part_has_grounded_evidence(&part_analysis, &retrieval.evidence)
            {
                continue;
            }
            let per_part_k = final_answer_k
                .saturating_div(plan.parts.len().max(1))
                .clamp(1, 3);
            let part_items = retrieval
                .evidence
                .into_iter()
                .take(per_part_k)
                .collect::<Vec<_>>();
            if !part_items.is_empty() {
                matched_parts += 1;
                selected.extend(part_items);
            }
        }

        if selected.is_empty() {
            selected.extend(root_evidence.iter().take(final_answer_k).cloned());
        }
        dedupe_evidence_preserve_order(&mut selected);
        selected.truncate(final_answer_k);

        Ok(CompoundEvidenceResult {
            evidence: selected,
            matched_parts,
            partial: matched_parts > 0 && matched_parts < plan.parts.len(),
        })
    }

    pub(crate) async fn retrieve_memory_context(
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


}
