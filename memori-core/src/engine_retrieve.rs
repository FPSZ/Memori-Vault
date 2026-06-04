use super::*;
use crate::engine::{memory_record_to_evidence, should_skip_memory_context};
use std::sync::atomic::{AtomicBool, Ordering};

/// 重排不可用时只 warn 一次，避免每次查询刷屏（默认开启但服务未起的常见场景）。
static RERANK_UNAVAILABLE_WARNED: AtomicBool = AtomicBool::new(false);

fn warn_rerank_unavailable_once(message: &str) {
    if !RERANK_UNAVAILABLE_WARNED.swap(true, Ordering::Relaxed) {
        warn!(
            detail = message,
            "rerank service unavailable; falling back to RRF order (further occurrences suppressed; set MEMORI_RERANK_ENABLED=0 to disable)"
        );
    } else {
        debug!(detail = message, "rerank unavailable; kept RRF order");
    }
}

/// 返回切片的 (min, max)；空或全非有限时返回 (0.0, 0.0)。
fn min_max(values: &[f64]) -> (f64, f64) {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for &value in values {
        if value < min {
            min = value;
        }
        if value > max {
            max = value;
        }
    }
    if min.is_finite() && max.is_finite() {
        (min, max)
    } else {
        (0.0, 0.0)
    }
}

/// min-max 归一化到 [0,1]；区间退化（全相等）时返回 0.5，避免除零并保持中性。
fn norm01(value: f64, min: f64, max: f64) -> f64 {
    let span = max - min;
    if span.abs() < 1e-9 {
        0.5
    } else {
        ((value - min) / span).clamp(0.0, 1.0)
    }
}

/// 重排融合：α·rerank + (1-α)·RRF，强结构信号（exact_path / scope）再加保护分。
fn fused_rerank_score(rerank_norm: f64, rrf_norm: f64, protected: bool) -> f64 {
    let base = RERANK_FUSION_ALPHA * rerank_norm + (1.0 - RERANK_FUSION_ALPHA) * rrf_norm;
    if protected {
        base + RERANK_PROTECT_BONUS
    } else {
        base
    }
}

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
            .resolve_candidate_documents(
                analysis,
                &query_embedding,
                normalized_scope_paths,
                metrics,
            )
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
        let mut evidence = merge_chunk_evidence(
            analysis,
            &candidate_docs,
            strict_lexical_matches,
            lexical_matches,
            dense_matches,
        );
        if evidence.is_empty() {
            evidence = self
                .fallback_candidate_doc_evidence(analysis, &candidate_docs)
                .await?;
        }
        metrics.merge_ms += elapsed_ms_u64(merge_started_at);
        metrics.chunk_candidate_count = metrics.chunk_candidate_count.max(evidence.len());
        debug!(
            merged_count = evidence.len(),
            ms = metrics.merge_ms,
            "evidence merge done"
        );

        self.rerank_merged_evidence(analysis, &mut evidence, metrics)
            .await;

        Ok(EvidenceRetrievalResult {
            candidate_docs,
            evidence,
        })
    }

    /// 召回后的 cross-encoder 重排（最高杠杆精度改进）。
    /// 取头部候选送本地重排服务，写回 rerank_score 并按 evidence_rank_cmp 重排。
    /// 未启用 / 服务不可达 / 返回异常时静默降级到 RRF 排序，绝不让检索失败。
    async fn rerank_merged_evidence(
        &self,
        analysis: &QueryAnalysis,
        evidence: &mut [MergedEvidence],
        metrics: &mut RetrievalMetrics,
    ) {
        if !self.state.rerank_client.is_enabled() || evidence.len() < 2 {
            return;
        }
        let query = analysis.normalized_query.trim();
        if query.is_empty() {
            return;
        }

        let rerank_started_at = Instant::now();
        let span = evidence.len().min(DEFAULT_RERANK_CANDIDATE_K);
        let documents = evidence[..span]
            .iter()
            .map(rerank_document_text)
            .collect::<Vec<_>>();

        match self.state.rerank_client.rerank(query, &documents).await {
            Ok(scores) if scores.len() == span => {
                // 融合而非覆盖：把 reranker 裸分与 RRF 分各自 min-max 归一化后加权，
                // 再对 exact_path / scope 等强结构信号加保护分，写回 rerank_score（排序首要键）。
                // 这样小 reranker 不会单凭一个"看起来更像答案"的命中把精确信号压下去。
                let finite_rerank = scores
                    .iter()
                    .map(|value| *value as f64)
                    .filter(|value| value.is_finite())
                    .collect::<Vec<_>>();
                let (rerank_min, rerank_max) = min_max(&finite_rerank);
                let rrf_scores = evidence[..span]
                    .iter()
                    .map(|item| item.final_score)
                    .collect::<Vec<_>>();
                let (rrf_min, rrf_max) = min_max(&rrf_scores);
                for (index, item) in evidence[..span].iter_mut().enumerate() {
                    let raw = scores[index] as f64;
                    let rerank_value = if raw.is_finite() { raw } else { rerank_min };
                    let rerank_norm = norm01(rerank_value, rerank_min, rerank_max);
                    let rrf_norm = norm01(item.final_score, rrf_min, rrf_max);
                    let protected =
                        item.document_reason == "scope" || item.document_has_exact_signal;
                    let fused = fused_rerank_score(rerank_norm, rrf_norm, protected);
                    item.rerank_score = Some(fused as f32);
                }
                evidence.sort_by(evidence_rank_cmp);
                metrics.rerank_ms += elapsed_ms_u64(rerank_started_at);
                metrics.query_flags.push(format!("rerank:applied:{span}"));
                debug!(
                    reranked = span,
                    ms = metrics.rerank_ms,
                    "rerank applied (fused)"
                );
            }
            Ok(scores) => {
                warn_rerank_unavailable_once(&format!(
                    "score count mismatch: expected={span}, actual={}",
                    scores.len()
                ));
                metrics
                    .query_flags
                    .push("rerank:skipped:count_mismatch".to_string());
            }
            Err(err) => {
                warn_rerank_unavailable_once(&err.to_string());
                metrics.query_flags.push("rerank:skipped:error".to_string());
            }
        }
    }

    async fn fallback_candidate_doc_evidence(
        &self,
        analysis: &QueryAnalysis,
        candidate_docs: &[DocumentCandidate],
    ) -> Result<Vec<MergedEvidence>, EngineError> {
        let mut evidence = Vec::new();
        for doc in candidate_docs.iter().take(3) {
            let chunk_records = self
                .state
                .vector_store
                .get_chunks_by_file_path(Path::new(&doc.file_path))
                .await?;
            for chunk in chunk_records.into_iter().take(4) {
                let document_chunk = DocumentChunk {
                    file_path: PathBuf::from(&doc.file_path),
                    content: chunk.content,
                    chunk_index: chunk.chunk_index,
                    heading_path: chunk.heading_path,
                    block_kind: parse_block_kind(&chunk.block_kind),
                };
                let lexical_signal = direct_chunk_lexical_signal(analysis, &document_chunk);
                let (lexical_strict_rank, lexical_broad_rank, lexical_raw_score, bonus_score) =
                    match lexical_signal {
                        Some((true, score)) => (Some(doc.document_rank), None, Some(score), 0.75),
                        Some((false, score)) => (None, Some(doc.document_rank), Some(score), 0.35),
                        None => {
                            if evidence.is_empty() && doc.document_rank == 1 {
                                (
                                    Some(DEFAULT_CHUNK_CANDIDATE_K + doc.document_rank),
                                    None,
                                    Some(0.5),
                                    0.2,
                                )
                            } else {
                                continue;
                            }
                        }
                    };
                evidence.push(MergedEvidence {
                    chunk: document_chunk,
                    relative_path: doc.relative_path.clone(),
                    document_reason: doc.document_reason.clone(),
                    document_rank: doc.document_rank,
                    document_raw_score: doc.document_raw_score,
                    document_has_exact_signal: doc.has_exact_signal,
                    document_has_docs_phrase_signal: doc.has_docs_phrase_signal,
                    document_docs_phrase_quality: doc.docs_phrase_quality,
                    document_has_filename_signal: doc.has_filename_signal,
                    document_has_strict_lexical: doc.has_strict_lexical,
                    lexical_strict_rank,
                    lexical_broad_rank,
                    lexical_raw_score,
                    dense_rank: None,
                    dense_raw_score: None,
                    final_score: doc.document_final_score + bonus_score,
                    rerank_score: None,
                });
            }
            if !evidence.is_empty() {
                break;
            }
        }
        evidence.sort_by(evidence_rank_cmp);
        Ok(evidence)
    }

    pub(crate) async fn retrieve_compound_evidence(
        &self,
        plan: &CompoundQueryPlan,
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
        query_embedding: &[f32],
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

        // 语义文档发现：在词法四路之外补一路 dense 召回，捞回"和 query 无 token 重叠
        // 但语义相近"的文档。一旦这些文档进了候选集，下游 scoped chunk 检索（含 dense）
        // 会自然覆盖它们——无需改 chunk 阶段，是最小侵入的解耦法。
        if !query_embedding.is_empty() {
            let doc_dense_started_at = Instant::now();
            let dense_hits = self
                .state
                .vector_store
                .search_similar_scoped(query_embedding.to_vec(), DENSE_DOC_TOP_K, scope_paths)
                .await?;
            // 每文件取最高 cos 分（及其排名），折叠成文档级候选。
            let mut best_by_file = HashMap::<String, (f32, usize)>::new();
            for (rank, (chunk, score)) in dense_hits.into_iter().enumerate() {
                let file_path = chunk.file_path.to_string_lossy().to_string();
                let entry = best_by_file.entry(file_path).or_insert((score, rank + 1));
                if score > entry.0 {
                    *entry = (score, rank + 1);
                }
            }
            for (file_path, (cos_score, rank)) in best_by_file {
                // 已被词法召回覆盖的文档不重复加（语义只补漏，不双计、不抢词法头部）。
                if by_path.contains_key(&file_path) {
                    continue;
                }
                let Some(record) = self
                    .state
                    .vector_store
                    .get_document_by_file_path(Path::new(&file_path))
                    .await?
                else {
                    continue;
                };
                let is_code_document = is_code_document_path(&record.relative_path);
                // RRF 风格小权重（系数 0.8 < broad lexical 的 1.0），只补召回不抢头部。
                let score = 0.8 / (RRF_K + rank as f64);
                by_path.insert(
                    record.file_path.clone(),
                    DocumentCandidate {
                        file_path: record.file_path,
                        relative_path: record.relative_path,
                        is_code_document,
                        document_reason: "semantic".to_string(),
                        document_rank: rank,
                        document_raw_score: Some(cos_score as f64),
                        exact_signal_score: None,
                        exact_path_score: None,
                        exact_symbol_score: None,
                        document_filename_score: None,
                        document_final_score: score,
                        has_exact_signal: false,
                        has_exact_path_signal: false,
                        has_exact_symbol_signal: false,
                        has_docs_phrase_signal: false,
                        docs_phrase_quality: None,
                        has_filename_signal: false,
                        has_strict_lexical: false,
                        has_broad_lexical: false,
                    },
                );
            }
            metrics.doc_dense_ms = elapsed_ms_u64(doc_dense_started_at);
        }

        let mut docs = by_path.into_values().collect::<Vec<_>>();
        rank_document_candidates_in_place(&mut docs, analysis.query_family);
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

#[cfg(test)]
mod rerank_fusion_tests {
    use super::{fused_rerank_score, min_max, norm01};

    #[test]
    fn min_max_handles_normal_and_degenerate_inputs() {
        assert_eq!(min_max(&[1.0, 3.0, 2.0]), (1.0, 3.0));
        assert_eq!(min_max(&[5.0]), (5.0, 5.0));
        // 空切片 / 全非有限退化为 (0,0)，下游 norm01 走 0.5 分支。
        assert_eq!(min_max(&[]), (0.0, 0.0));
    }

    #[test]
    fn norm01_returns_neutral_on_zero_span() {
        assert_eq!(norm01(5.0, 5.0, 5.0), 0.5);
        assert_eq!(norm01(0.0, 4.0, 4.0), 0.5);
        assert_eq!(norm01(2.0, 0.0, 4.0), 0.5);
        assert_eq!(norm01(4.0, 0.0, 4.0), 1.0);
        // 越界 clamp 到 [0,1]。
        assert_eq!(norm01(8.0, 0.0, 4.0), 1.0);
    }

    #[test]
    fn protect_bonus_lets_strong_signal_resist_a_higher_reranker_hit() {
        // 弱 rerank(0.0) 但强结构信号(scope/exact_path) → 有保护分
        let protected = fused_rerank_score(0.0, 1.0, true);
        // 较高 rerank(0.4) 但无保护
        let unprotected = fused_rerank_score(0.4, 0.0, false);
        assert!(
            protected > unprotected,
            "protected={protected} should beat unprotected={unprotected}"
        );
    }

    #[test]
    fn fusion_blends_both_signals() {
        // α=0.7：纯 rerank 满分 = 0.7；纯 RRF 满分 = 0.3；两者都满 = 1.0。
        assert!((fused_rerank_score(1.0, 0.0, false) - 0.7).abs() < 1e-9);
        assert!((fused_rerank_score(0.0, 1.0, false) - 0.3).abs() < 1e-9);
        assert!((fused_rerank_score(1.0, 1.0, false) - 1.0).abs() < 1e-9);
    }
}
