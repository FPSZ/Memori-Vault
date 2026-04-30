use super::*;
use crate::engine::answer_indicates_insufficient_evidence;

impl MemoriEngine {

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
        let mut answer_question = build_answer_question(&inspection.question, lang);
        if detect_compound_query(&inspection.question).is_some() {
            answer_question.push_str(
                "\n\n多项目/多主题回答要求：请按项目或主题分段回答；每个项目只使用对应证据，不要把不同项目的事实混在一起。如果某个项目证据不足，请单独说明该项目缺少证据。",
            );
        }
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
        let compound_plan = detect_compound_query(&question);
        if let Some(plan) = compound_plan.as_ref() {
            metrics.query_flags.push("compound_query:true".to_string());
            metrics
                .query_flags
                .push(format!("compound_parts:{}", plan.parts.len()));
        }

        let should_embed_compound_parts = !query_embedding.is_empty();
        let retrieval = self
            .retrieve_evidence_for_analysis(
                &analysis,
                query_embedding,
                &normalized_scope_paths,
                &mut metrics,
            )
            .await?;
        let candidate_docs = retrieval.candidate_docs;
        let mut merged = retrieval.evidence;

        if let Some(plan) = compound_plan.as_ref() {
            let compound_result = self
                .retrieve_compound_evidence(
                    plan,
                    &analysis,
                    &merged,
                    &normalized_scope_paths,
                    final_answer_k,
                    should_embed_compound_parts,
                    &mut metrics,
                )
                .await?;
            if !compound_result.evidence.is_empty() {
                merged = compound_result.evidence;
            }
            metrics
                .query_flags
                .push(format!("compound_partial:{}", compound_result.partial));
            if compound_result.matched_parts > 0 {
                metrics.gating_decision_reason =
                    if compound_result.matched_parts == plan.parts.len() {
                        "compound_evidence_release".to_string()
                    } else {
                        "compound_partial_release".to_string()
                    };
                let final_evidence = merged.into_iter().take(final_answer_k).collect::<Vec<_>>();
                metrics.final_evidence_count = final_evidence.len();
                let citations = build_citations(&final_evidence);
                let evidence_items = build_evidence_items(&final_evidence);
                return Ok(RetrievalInspection {
                    status: AskStatus::Answered,
                    question,
                    scope_paths: serialized_scope_paths,
                    source_groups: build_source_groups(&citations, &evidence_items),
                    citations,
                    evidence: evidence_items,
                    metrics,
                    answer_source_mix: if memory_context.is_empty() {
                        AnswerSourceMix::DocumentOnly
                    } else {
                        AnswerSourceMix::DocumentPlusMemory
                    },
                    memory_context,
                    failure_class: FailureClass::None,
                    context_budget_report: ContextBudgetReport::default(),
                });
            }
            metrics.gating_decision_reason = "compound_all_missing".to_string();
        }

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


}
