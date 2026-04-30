
    use super::{
        AppState, AskStatus, EgressMode, EngineError, EnterpriseModelPolicy, MemoriEngine,
        MemoryEvidence, MemoryLayer, MemoryScope, MemorySourceType, MemoryStatus, MergedEvidence,
        ModelProvider, QueryFamily, QueryIntent, RetrievalMetrics, RuntimeModelConfig, WatchEvent,
        WatchEventKind, analyze_query, apply_gating_metrics, build_citations,
        build_memory_context_for_prompt, detect_compound_query, document_signal_query,
        has_strong_document_signal, is_implementation_lookup, merge_document_candidates,
        process_file_event, should_allow_memory_only_answer,
        should_refuse_for_insufficient_evidence, validate_runtime_model_settings,
    };
    use memori_parser::DocumentChunk;
    use memori_storage::RebuildState;
    use memori_storage::VectorStore;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("memori_vault_core_{name}_{unique}.db"))
    }

    async fn seed_indexed_file(state: &Arc<AppState>, file_path: &PathBuf) {
        state
            .vector_store
            .insert_chunks(
                vec![DocumentChunk {
                    file_path: file_path.clone(),
                    content: "seed content".to_string(),
                    chunk_index: 0,
                    heading_path: Vec::new(),
                    block_kind: memori_parser::ChunkBlockKind::Paragraph,
                }],
                vec![vec![0.1_f32, 0.2_f32]],
            )
            .await
            .expect("insert seed chunks");
        state
            .vector_store
            .upsert_file_index_state(file_path, 12, 34, "seed_hash")
            .await
            .expect("upsert seed index state");
    }

    async fn seed_document_chunks(
        state: &Arc<AppState>,
        file_path: &PathBuf,
        chunks: Vec<DocumentChunk>,
    ) {
        let embeddings = vec![vec![0.1_f32, 0.2_f32]; chunks.len()];
        state
            .vector_store
            .replace_document_index(file_path, None, 123, "test_hash", chunks, embeddings)
            .await
            .expect("replace document index");
    }

    #[tokio::test]
    async fn removed_event_purges_existing_index() {
        let db_path = temp_db_path("removed");
        let state = Arc::new(AppState::new(&db_path).expect("create app state"));
        let file_path = PathBuf::from("notes/removed.md");
        seed_indexed_file(&state, &file_path).await;

        let event = WatchEvent {
            kind: WatchEventKind::Removed,
            path: file_path.clone(),
            old_path: None,
            observed_at: SystemTime::now(),
        };

        process_file_event(&state, &event, None, None, false).await;

        assert!(
            state
                .vector_store
                .resolve_chunk_id(&file_path, 0)
                .await
                .expect("resolve chunk after remove")
                .is_none()
        );
        assert!(
            state
                .vector_store
                .get_file_index_state(&file_path)
                .await
                .expect("get file index after remove")
                .is_none()
        );

        drop(state);
        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn retrieval_uses_chunk_fts_for_matched_cjk_identifier_document() {
        let db_path = temp_db_path("cjk_chunk_fts");
        let state = Arc::new(AppState::new(&db_path).expect("create app state"));
        let file_path = PathBuf::from("Memory_Test/doc_017_技术架构_银杏-17_会议纪要.md");
        seed_document_chunks(
            &state,
            &file_path,
            vec![
                DocumentChunk {
                    file_path: file_path.clone(),
                    content: "会议背景：平台架构组讨论上线安排。".to_string(),
                    chunk_index: 0,
                    heading_path: vec!["会议纪要".to_string()],
                    block_kind: memori_parser::ChunkBlockKind::Paragraph,
                },
                DocumentChunk {
                    file_path: file_path.clone(),
                    content: "- 项目代号：银杏-17 / ARC-17\n- 负责人：苏澈（平台架构组）\n- 核心事实：银杏-17 的上线冻结窗口是每周三 19:40-21:10。".to_string(),
                    chunk_index: 1,
                    heading_path: vec!["关键结论".to_string()],
                    block_kind: memori_parser::ChunkBlockKind::List,
                },
            ],
        )
        .await;

        let engine = MemoriEngine::new(state.clone(), memori_vault::create_event_channel().1);
        let inspection = engine
            .retrieve_structured_with_embedding("银杏-17的负责人是谁", Vec::new(), None, Some(3))
            .await
            .expect("retrieve structured");

        assert_eq!(inspection.status, AskStatus::Answered);
        assert!(
            inspection
                .evidence
                .iter()
                .any(|item| item.content.contains("负责人：苏澈"))
        );
        assert!(inspection.metrics.chunk_candidate_count > 0);

        drop(state);
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn compound_query_detection_splits_specific_project_topics() {
        let plan =
            detect_compound_query("极光账本 和 雾凇发布 的负责人分别是谁").expect("compound plan");

        assert_eq!(plan.parts.len(), 2);
        assert_eq!(plan.parts[0].topic, "极光账本");
        assert_eq!(plan.parts[1].topic, "雾凇发布");
        assert!(plan.parts[0].query.contains("负责人"));
        assert!(detect_compound_query("极光账本-17 的负责人是谁").is_none());
    }

    #[tokio::test]
    async fn compound_query_retrieves_evidence_for_multiple_projects() {
        let db_path = temp_db_path("compound_projects");
        let state = Arc::new(AppState::new(&db_path).expect("create app state"));
        let aurora_path = PathBuf::from("Memory_Test/doc_001_极光账本-17_会议纪要.md");
        let rime_path = PathBuf::from("Memory_Test/doc_002_雾凇发布-04_问答卡.md");
        seed_document_chunks(
            &state,
            &aurora_path,
            vec![
                DocumentChunk {
                    file_path: aurora_path.clone(),
                    content: "项目代号：极光账本-17。负责人：林澈。当前风险：预算审批延迟。"
                        .to_string(),
                    chunk_index: 0,
                    heading_path: vec!["项目概况".to_string()],
                    block_kind: memori_parser::ChunkBlockKind::Paragraph,
                },
                DocumentChunk {
                    file_path: aurora_path.clone(),
                    content: "极光账本-17 的验收要求是完成财务流水核验和权限审计。".to_string(),
                    chunk_index: 1,
                    heading_path: vec!["验收".to_string()],
                    block_kind: memori_parser::ChunkBlockKind::Paragraph,
                },
            ],
        )
        .await;
        seed_document_chunks(
            &state,
            &rime_path,
            vec![
                DocumentChunk {
                    file_path: rime_path.clone(),
                    content: "项目代号：雾凇发布-04。负责人：周岚。当前风险：发布物料冻结较晚。"
                        .to_string(),
                    chunk_index: 0,
                    heading_path: vec!["项目概况".to_string()],
                    block_kind: memori_parser::ChunkBlockKind::Paragraph,
                },
                DocumentChunk {
                    file_path: rime_path.clone(),
                    content: "雾凇发布-04 的验收要求是完成市场问答卡和品牌检查清单。".to_string(),
                    chunk_index: 1,
                    heading_path: vec!["验收".to_string()],
                    block_kind: memori_parser::ChunkBlockKind::Paragraph,
                },
            ],
        )
        .await;

        let engine = MemoriEngine::new(state.clone(), memori_vault::create_event_channel().1);
        let inspection = engine
            .retrieve_structured_with_embedding(
                "极光账本-17 和 雾凇发布-04 的负责人分别是谁",
                Vec::new(),
                None,
                Some(6),
            )
            .await
            .expect("retrieve structured");

        assert_eq!(inspection.status, AskStatus::Answered);
        assert_eq!(
            inspection.metrics.gating_decision_reason,
            "compound_evidence_release"
        );
        assert!(
            inspection
                .metrics
                .query_flags
                .contains(&"compound_query:true".to_string())
        );
        assert!(
            inspection
                .evidence
                .iter()
                .any(|item| item.content.contains("负责人：林澈"))
        );
        assert!(
            inspection
                .evidence
                .iter()
                .any(|item| item.content.contains("负责人：周岚"))
        );

        drop(state);
        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn compound_query_allows_partial_project_evidence() {
        let db_path = temp_db_path("compound_partial");
        let state = Arc::new(AppState::new(&db_path).expect("create app state"));
        let aurora_path = PathBuf::from("Memory_Test/doc_003_极光账本-17_制度.md");
        seed_document_chunks(
            &state,
            &aurora_path,
            vec![
                DocumentChunk {
                    file_path: aurora_path.clone(),
                    content: "项目代号：极光账本-17。负责人：林澈。".to_string(),
                    chunk_index: 0,
                    heading_path: vec!["项目概况".to_string()],
                    block_kind: memori_parser::ChunkBlockKind::Paragraph,
                },
                DocumentChunk {
                    file_path: aurora_path.clone(),
                    content: "极光账本-17 的当前风险是预算审批延迟。".to_string(),
                    chunk_index: 1,
                    heading_path: vec!["风险".to_string()],
                    block_kind: memori_parser::ChunkBlockKind::Paragraph,
                },
            ],
        )
        .await;

        let engine = MemoriEngine::new(state.clone(), memori_vault::create_event_channel().1);
        let inspection = engine
            .retrieve_structured_with_embedding(
                "极光账本-17 和 不存在项目-99 的负责人分别是谁",
                Vec::new(),
                None,
                Some(6),
            )
            .await
            .expect("retrieve structured");

        assert_eq!(inspection.status, AskStatus::Answered);
        assert!(
            inspection
                .metrics
                .query_flags
                .contains(&"compound_query:true".to_string())
        );
        assert!(
            inspection
                .metrics
                .query_flags
                .contains(&"compound_partial:true".to_string())
        );
        assert!(
            inspection
                .evidence
                .iter()
                .any(|item| item.content.contains("负责人：林澈"))
        );

        drop(state);
        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn compound_query_can_answer_when_root_query_has_no_candidates() {
        let db_path = temp_db_path("compound_no_root");
        let state = Arc::new(AppState::new(&db_path).expect("create app state"));
        let aurora_path = PathBuf::from("Memory_Test/doc_004_极光账本_内部资料.md");
        seed_document_chunks(
            &state,
            &aurora_path,
            vec![
                DocumentChunk {
                    file_path: aurora_path.clone(),
                    content: "极光账本 的负责人是林知远，关键指标是月度已核销对账单数。"
                        .to_string(),
                    chunk_index: 0,
                    heading_path: vec!["项目概况".to_string()],
                    block_kind: memori_parser::ChunkBlockKind::Paragraph,
                },
                DocumentChunk {
                    file_path: aurora_path.clone(),
                    content: "极光账本 的内部规定要求试点客户限定为三家。".to_string(),
                    chunk_index: 1,
                    heading_path: vec!["内部规定".to_string()],
                    block_kind: memori_parser::ChunkBlockKind::Paragraph,
                },
            ],
        )
        .await;

        let engine = MemoriEngine::new(state.clone(), memori_vault::create_event_channel().1);
        let inspection = engine
            .retrieve_structured_with_embedding(
                "极光账本和不存在项目-99的负责人分别是谁",
                Vec::new(),
                None,
                Some(6),
            )
            .await
            .expect("retrieve structured");

        assert_eq!(inspection.status, AskStatus::Answered);
        assert!(
            inspection
                .metrics
                .query_flags
                .contains(&"compound_query:true".to_string())
        );
        assert!(
            inspection
                .evidence
                .iter()
                .any(|item| item.content.contains("林知远"))
        );

        drop(state);
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn build_citations_dedupes_identical_rendered_excerpt_from_same_file() {
        let file_path = std::env::temp_dir().join(format!(
            "memori_citation_excerpt_{}.md",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time before unix epoch")
                .as_nanos()
        ));
        let content = "Project note: query_analysis_ms is emitted in retrieval metrics, and query_analysis_ms should stay visible to users for debugging.\n\nSecond paragraph stays separate.";
        fs::write(&file_path, content).expect("write temp markdown");

        let evidence = vec![
            MergedEvidence {
                chunk: DocumentChunk {
                    file_path: file_path.clone(),
                    content: "query_analysis_ms is emitted in retrieval metrics".to_string(),
                    chunk_index: 0,
                    heading_path: vec!["Metrics".to_string()],
                    block_kind: memori_parser::ChunkBlockKind::Paragraph,
                },
                relative_path: "notes/metrics.md".to_string(),
                document_reason: "lexical_strict".to_string(),
                document_rank: 1,
                document_raw_score: Some(1.0),
                document_has_exact_signal: false,
                document_has_docs_phrase_signal: false,
                document_docs_phrase_quality: None,
                document_has_filename_signal: false,
                document_has_strict_lexical: true,
                lexical_strict_rank: Some(1),
                lexical_broad_rank: None,
                lexical_raw_score: Some(1.0),
                dense_rank: None,
                dense_raw_score: None,
                final_score: 1.0,
            },
            MergedEvidence {
                chunk: DocumentChunk {
                    file_path: file_path.clone(),
                    content: "query_analysis_ms should stay visible to users".to_string(),
                    chunk_index: 1,
                    heading_path: vec!["Metrics".to_string()],
                    block_kind: memori_parser::ChunkBlockKind::Paragraph,
                },
                relative_path: "notes/metrics.md".to_string(),
                document_reason: "mixed".to_string(),
                document_rank: 1,
                document_raw_score: Some(0.9),
                document_has_exact_signal: false,
                document_has_docs_phrase_signal: false,
                document_docs_phrase_quality: None,
                document_has_filename_signal: false,
                document_has_strict_lexical: true,
                lexical_strict_rank: Some(2),
                lexical_broad_rank: None,
                lexical_raw_score: Some(0.9),
                dense_rank: Some(1),
                dense_raw_score: Some(0.8),
                final_score: 0.95,
            },
        ];

        let citations = build_citations(&evidence);

        assert_eq!(citations.len(), 1);
        assert_eq!(citations[0].index, 1);
        assert_eq!(citations[0].relative_path, "notes/metrics.md");

        let _ = fs::remove_file(file_path);
    }

    #[test]
    fn local_only_blocks_remote_runtime() {
        let policy = EnterpriseModelPolicy {
            egress_mode: EgressMode::LocalOnly,
            allowed_model_endpoints: Vec::new(),
            allowed_models: Vec::new(),
        };
        let runtime = RuntimeModelConfig {
            provider: ModelProvider::OpenAiCompatible,
            chat_endpoint: "https://api.openai.com/v1".to_string(),
            chat_model: "gpt-4o-mini".to_string(),
            graph_endpoint: "https://api.openai.com/v1".to_string(),
            graph_model: "gpt-4o-mini".to_string(),
            embed_endpoint: "https://api.openai.com/v1".to_string(),
            embed_model: "text-embedding-3-small".to_string(),
            api_key: Some("secret".to_string()),
            chat_context_length: None,
            graph_context_length: None,
            embed_context_length: None,
            chat_concurrency: None,
            graph_concurrency: None,
            embed_concurrency: None,
        };

        let violation = validate_runtime_model_settings(&policy, &runtime)
            .expect_err("remote runtime should be blocked");
        assert_eq!(violation.code, "runtime_blocked_by_policy");
    }

    #[test]
    fn allowlist_requires_endpoint_and_models() {
        let policy = EnterpriseModelPolicy {
            egress_mode: EgressMode::Allowlist,
            allowed_model_endpoints: vec!["https://models.company.local/v1/".to_string()],
            allowed_models: vec!["approved-chat".to_string(), "approved-embed".to_string()],
        };
        let runtime = RuntimeModelConfig {
            provider: ModelProvider::OpenAiCompatible,
            chat_endpoint: "https://models.company.local/v1".to_string(),
            chat_model: "approved-chat".to_string(),
            graph_endpoint: "https://models.company.local/v1".to_string(),
            graph_model: "approved-chat".to_string(),
            embed_endpoint: "https://models.company.local/v1".to_string(),
            embed_model: "denied-embed".to_string(),
            api_key: None,
            chat_context_length: None,
            graph_context_length: None,
            embed_context_length: None,
            chat_concurrency: None,
            graph_concurrency: None,
            embed_concurrency: None,
        };

        let violation = validate_runtime_model_settings(&policy, &runtime)
            .expect_err("non-allowlisted model should be blocked");
        assert_eq!(violation.code, "model_not_allowlisted");
    }

    #[tokio::test]
    async fn renamed_event_to_unsupported_extension_only_purges_old_index() {
        let db_path = temp_db_path("rename_unsupported");
        let state = Arc::new(AppState::new(&db_path).expect("create app state"));
        let old_path = PathBuf::from("notes/rename_me.md");
        let new_path = PathBuf::from("notes/rename_me.pdf");
        seed_indexed_file(&state, &old_path).await;

        let event = WatchEvent {
            kind: WatchEventKind::Renamed,
            path: new_path.clone(),
            old_path: Some(old_path.clone()),
            observed_at: SystemTime::now(),
        };

        process_file_event(&state, &event, None, None, false).await;

        assert!(
            state
                .vector_store
                .resolve_chunk_id(&old_path, 0)
                .await
                .expect("resolve old chunk after rename")
                .is_none()
        );
        assert!(
            state
                .vector_store
                .get_file_index_state(&old_path)
                .await
                .expect("get old file index after rename")
                .is_none()
        );
        assert!(
            state
                .vector_store
                .get_file_index_state(&new_path)
                .await
                .expect("get new file index after rename")
                .is_none()
        );

        drop(state);
        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn removed_directory_event_purges_nested_indexes() {
        let db_path = temp_db_path("removed_dir");
        let state = Arc::new(AppState::new(&db_path).expect("create app state"));
        let nested_a = PathBuf::from("notes/project/a.md");
        let nested_b = PathBuf::from("notes/project/sub/b.txt");
        let outside = PathBuf::from("notes/other/c.md");

        seed_indexed_file(&state, &nested_a).await;
        seed_indexed_file(&state, &nested_b).await;
        seed_indexed_file(&state, &outside).await;

        let event = WatchEvent {
            kind: WatchEventKind::Removed,
            path: PathBuf::from("notes/project"),
            old_path: None,
            observed_at: SystemTime::now(),
        };

        process_file_event(&state, &event, None, None, false).await;

        assert!(
            state
                .vector_store
                .resolve_chunk_id(&nested_a, 0)
                .await
                .expect("resolve nested a")
                .is_none()
        );
        assert!(
            state
                .vector_store
                .resolve_chunk_id(&nested_b, 0)
                .await
                .expect("resolve nested b")
                .is_none()
        );
        assert!(
            state
                .vector_store
                .resolve_chunk_id(&outside, 0)
                .await
                .expect("resolve outside")
                .is_some()
        );

        drop(state);
        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn unchanged_metadata_branch_restores_idle_phase() {
        let db_path = temp_db_path("unchanged_meta");
        let state = Arc::new(AppState::new(&db_path).expect("create app state"));
        let file_path = PathBuf::from("notes/unchanged.md");

        let parent = std::env::temp_dir().join(format!(
            "memori_vault_core_file_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("duration since epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&parent).expect("create temp dir");
        let real_file_path = parent.join("unchanged.md");
        std::fs::write(&real_file_path, "same content").expect("write temp file");

        let metadata = std::fs::metadata(&real_file_path).expect("read metadata");
        let file_size = i64::try_from(metadata.len()).expect("file size fits i64");
        let mtime_secs = metadata
            .modified()
            .expect("modified time")
            .duration_since(UNIX_EPOCH)
            .expect("duration since epoch")
            .as_secs() as i64;
        state
            .vector_store
            .upsert_file_index_state(&real_file_path, file_size, mtime_secs, "seed_hash")
            .await
            .expect("seed file index state");

        let event = WatchEvent {
            kind: WatchEventKind::Modified,
            path: real_file_path.clone(),
            old_path: Some(file_path),
            observed_at: SystemTime::now(),
        };

        process_file_event(&state, &event, None, None, false).await;

        let runtime = state.indexing_runtime.read().await.clone();
        assert_eq!(runtime.phase, "idle");

        drop(state);
        let _ = std::fs::remove_file(&real_file_path);
        let _ = std::fs::remove_dir_all(&parent);
        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn search_is_blocked_when_index_rebuild_is_required() {
        let db_path = temp_db_path("search_blocked_required");
        let state = Arc::new(AppState::new(&db_path).expect("create app state"));
        state
            .vector_store
            .mark_rebuild_required("parser_format_changed")
            .await
            .expect("mark rebuild required");

        let (_tx, rx) = tokio::sync::mpsc::channel(8);
        let engine = MemoriEngine::new(state, rx);
        let err = engine
            .search("test query", 5, None)
            .await
            .expect_err("search should be blocked");

        assert!(matches!(err, EngineError::IndexUnavailable { .. }));

        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn retrieve_structured_empty_query_returns_insufficient_evidence() {
        let db_path = temp_db_path("retrieve_empty");
        let state = Arc::new(AppState::new(&db_path).expect("create app state"));
        let (_tx, rx) = tokio::sync::mpsc::channel(8);
        let engine = MemoriEngine::new(state, rx);

        let response = engine
            .retrieve_structured("   ", None, None)
            .await
            .expect("retrieve structured");

        assert_eq!(response.status, AskStatus::InsufficientEvidence);
        assert!(response.citations.is_empty());
        assert!(response.evidence.is_empty());

        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn daemon_rebuilds_required_index_and_returns_ready() {
        let db_path = temp_db_path("daemon_rebuild_required");
        let state = Arc::new(AppState::new(&db_path).expect("create app state"));
        state
            .vector_store
            .mark_rebuild_required("parser_format_changed")
            .await
            .expect("mark rebuild required");

        let temp_root = std::env::temp_dir().join(format!(
            "memori_vault_core_rebuild_root_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("duration since epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_root).expect("create temp root");

        let (tx, rx) = tokio::sync::mpsc::channel(8);
        drop(tx);

        let mut engine = MemoriEngine::new(state.clone(), rx);
        engine.watch_root = Some(temp_root.clone());
        engine.start_daemon().expect("start daemon");
        engine.shutdown().await.expect("shutdown daemon");

        let metadata = state
            .vector_store
            .read_index_metadata()
            .await
            .expect("read index metadata");
        assert_eq!(metadata.rebuild_state, RebuildState::Ready);
        assert!(metadata.rebuild_reason.is_none());

        drop(state);
        let _ = std::fs::remove_dir_all(&temp_root);
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn analyze_query_keeps_cjk_phrase_and_identifier_terms() {
        let analysis = analyze_query("长跳转公式是什么 POST /api/ask 周报8 week8_report.md");
        assert!(analysis.chunk_terms.iter().any(|term| term == "长跳转公式"));
        assert!(!analysis.chunk_terms.iter().any(|term| term == "是什么"));
        assert!(
            analysis
                .identifier_terms
                .iter()
                .any(|term| term == "week8_report.md")
        );
        assert!(analysis.identifier_terms.iter().any(|term| term == "api"));
        assert!(
            analysis
                .filename_like_terms
                .iter()
                .any(|term| term == "周报8")
        );
        assert!(analysis.flags.has_cjk);
        assert!(analysis.flags.has_path_like_token);
        assert!(analysis.flags.is_lookup_like);
    }

    #[test]
    fn analyze_query_extracts_generic_cjk_backoff_terms() {
        let founded = analyze_query("北极星生物计算成立于");
        assert!(
            founded
                .chunk_terms
                .iter()
                .any(|term| term == "北极星生物计算")
        );

        let description = analyze_query("星海系统是做什么的");
        assert!(
            description
                .chunk_terms
                .iter()
                .any(|term| term == "星海系统")
        );

        let short_entity = analyze_query("腾讯成立于");
        assert!(short_entity.chunk_terms.iter().any(|term| term == "腾讯"));
    }

    #[test]
    fn analyze_query_splits_mixed_script_entity_boundaries() {
        let analysis = analyze_query("北极星生物计算PolarisBioCompute成立于");
        assert!(
            analysis
                .chunk_terms
                .iter()
                .any(|term| term == "北极星生物计算")
        );
        assert!(
            analysis
                .chunk_terms
                .iter()
                .any(|term| term == "polarisbiocompute")
        );

        let reverse = analyze_query("PolarisBioCompute北极星生物计算");
        assert!(
            reverse
                .chunk_terms
                .iter()
                .any(|term| term == "北极星生物计算")
        );
        assert!(
            reverse
                .chunk_terms
                .iter()
                .any(|term| term == "polarisbiocompute")
        );
    }

    #[test]
    fn analyze_query_extracts_support_terms_for_descriptive_cjk_questions() {
        let analysis = analyze_query("新增的岗位是什么");
        assert!(analysis.support_terms.iter().any(|term| term == "新增"));
        assert!(analysis.support_terms.iter().any(|term| term == "岗位"));
        assert!(!analysis.support_terms.iter().any(|term| term == "是什么"));
        assert!(
            !analysis
                .support_terms
                .iter()
                .any(|term| term == "新增的岗位是什么")
        );
    }

    #[test]
    fn classify_query_intent_marks_external_and_secret_queries() {
        assert_eq!(
            analyze_query("CEO of OpenAI").query_intent,
            QueryIntent::ExternalFact
        );
        assert_eq!(
            analyze_query("Bitcoin price today").query_intent,
            QueryIntent::ExternalFact
        );
        assert_eq!(
            analyze_query("hidden remote API key").query_intent,
            QueryIntent::SecretRequest
        );
    }

    #[test]
    fn memory_intent_only_allows_explicit_memory_questions() {
        let document_question = analyze_query("What does the onboarding document say?");
        let memory_question = analyze_query("What did I say about my project preference earlier?");
        let memory = MemoryEvidence {
            id: 1,
            layer: MemoryLayer::Ltm,
            scope: MemoryScope::Project,
            memory_type: "preference".to_string(),
            title: "Language".to_string(),
            content: "Prefer concise Chinese answers.".to_string(),
            source_type: MemorySourceType::ConversationTurn,
            source_ref: "conversation_turn:test".to_string(),
            confidence: 0.9,
            status: MemoryStatus::Active,
        };

        assert!(!should_allow_memory_only_answer(
            &document_question,
            std::slice::from_ref(&memory)
        ));
        assert!(should_allow_memory_only_answer(&memory_question, &[memory]));
    }

    #[test]
    fn memory_prompt_context_reports_token_budget() {
        let memory = MemoryEvidence {
            id: 7,
            layer: MemoryLayer::Mtm,
            scope: MemoryScope::Project,
            memory_type: "decision".to_string(),
            title: "Architecture decision".to_string(),
            content: "Graph stays evidence-only and does not affect main retrieval ranking."
                .to_string(),
            source_type: MemorySourceType::ToolEvent,
            source_ref: "tool_event:test".to_string(),
            confidence: 0.85,
            status: MemoryStatus::Active,
        };

        let (context, tokens) = build_memory_context_for_prompt(&[memory], 1_000);
        assert!(context.contains("Memory #7"));
        assert!(context.contains("source_ref: tool_event:test"));
        assert!(tokens > 0);
    }

    #[test]
    fn classify_query_family_distinguishes_docs_api_and_implementation_queries() {
        assert_eq!(
            analyze_query("POST /api/auth/oidc/login return?").query_family,
            QueryFamily::DocsApiLookup
        );
        assert_eq!(
            analyze_query("GET /api/admin/metrics").query_family,
            QueryFamily::DocsApiLookup
        );
        assert_eq!(
            analyze_query("ask_vault_structured 是哪个入口？").query_family,
            QueryFamily::ImplementationLookup
        );
        assert_eq!(
            analyze_query("How do you start server mode?").query_family,
            QueryFamily::DocsExplanatory
        );
    }

    #[test]
    fn document_signal_query_keeps_docs_terms_for_explanatory_queries() {
        let analysis = analyze_query("What does the tutorial say if vault stats stay at 0?");
        let query = document_signal_query(&analysis);
        assert!(!query.trim().is_empty());
        assert!(query.contains("tutorial"));
        assert!(query.contains("stats"));
    }

    #[test]
    fn document_merge_prefers_filename_signal_when_scores_are_stronger() {
        let analysis = analyze_query("week8_report.md");
        let merged = merge_document_candidates(
            &analysis,
            vec![memori_storage::DocumentSignalMatch {
                file_path: "docs/week8_report.md".to_string(),
                relative_path: "docs/week8_report.md".to_string(),
                file_name: "week8_report.md".to_string(),
                matched_fields: vec!["file_name".to_string()],
                score: 120,
                phrase_specific: false,
            }],
            Vec::new(),
            Vec::new(),
            vec![memori_storage::FtsDocumentMatch {
                doc_id: 1,
                file_path: "docs/other.md".to_string(),
                relative_path: "docs/other.md".to_string(),
                file_name: "other.md".to_string(),
                score: 0.4,
                heading_catalog_text: String::new(),
                document_search_text: String::new(),
            }],
        );

        assert_eq!(merged[0].file_name, "week8_report.md");
        assert_eq!(merged[0].document_reason, "filename");
    }

    #[test]
    fn document_merge_prefers_exact_symbol_for_implementation_lookup() {
        let analysis = analyze_query("ask_vault_structured");
        let merged = merge_document_candidates(
            &analysis,
            vec![memori_storage::DocumentSignalMatch {
                file_path: "memori-desktop/src/lib.rs".to_string(),
                relative_path: "memori-desktop/src/lib.rs".to_string(),
                file_name: "lib.rs".to_string(),
                matched_fields: vec!["exact_symbol".to_string()],
                score: 160,
                phrase_specific: false,
            }],
            Vec::new(),
            Vec::new(),
            vec![memori_storage::FtsDocumentMatch {
                doc_id: 1,
                file_path: "README.md".to_string(),
                relative_path: "README.md".to_string(),
                file_name: "README.md".to_string(),
                score: 3.0,
                heading_catalog_text: String::new(),
                document_search_text: "ask vault structured".to_string(),
            }],
        );

        assert_eq!(merged[0].relative_path, "memori-desktop/src/lib.rs");
        assert_eq!(merged[0].document_reason, "exact_symbol");
    }

    #[test]
    fn document_merge_prefers_docs_phrase_for_docs_api_lookup() {
        let analysis = analyze_query("POST /api/auth/oidc/login return?");
        let merged = merge_document_candidates(
            &analysis,
            Vec::new(),
            vec![memori_storage::DocumentSignalMatch {
                file_path: "docs/guides/enterprise.md".to_string(),
                relative_path: "docs/guides/enterprise.md".to_string(),
                file_name: "enterprise.md".to_string(),
                matched_fields: vec!["docs_phrase".to_string()],
                score: 180,
                phrase_specific: true,
            }],
            Vec::new(),
            vec![memori_storage::FtsDocumentMatch {
                doc_id: 1,
                file_path: "memori-server/src/main.rs".to_string(),
                relative_path: "memori-server/src/main.rs".to_string(),
                file_name: "main.rs".to_string(),
                score: 5.0,
                heading_catalog_text: String::new(),
                document_search_text: "POST /api/auth/oidc/login".to_string(),
            }],
        );

        assert_eq!(merged[0].relative_path, "docs/guides/enterprise.md");
        assert_eq!(merged[0].document_reason, "docs_phrase");
    }

    #[test]
    fn document_merge_demotes_generic_docs_phrase_for_docs_queries() {
        let analysis = analyze_query("岗位是什么");
        let merged = merge_document_candidates(
            &analysis,
            Vec::new(),
            vec![memori_storage::DocumentSignalMatch {
                file_path: "docs/overview.md".to_string(),
                relative_path: "docs/overview.md".to_string(),
                file_name: "overview.md".to_string(),
                matched_fields: vec!["docs_phrase".to_string()],
                score: 120,
                phrase_specific: false,
            }],
            vec![memori_storage::FtsDocumentMatch {
                doc_id: 1,
                file_path: "docs/hiring.md".to_string(),
                relative_path: "docs/hiring.md".to_string(),
                file_name: "hiring.md".to_string(),
                score: 1.2,
                heading_catalog_text: "招聘计划".to_string(),
                document_search_text: "新增 12 个岗位".to_string(),
            }],
            Vec::new(),
        );

        assert_eq!(merged[0].relative_path, "docs/hiring.md");
        assert_eq!(merged[0].document_reason, "lexical_strict");
    }

    #[test]
    fn generic_docs_phrase_is_not_treated_as_strong_signal() {
        let item = MergedEvidence {
            chunk: DocumentChunk {
                file_path: PathBuf::from("docs/overview.md"),
                content: "岗位概览".to_string(),
                chunk_index: 0,
                heading_path: vec!["概览".to_string()],
                block_kind: memori_parser::ChunkBlockKind::Paragraph,
            },
            relative_path: "docs/overview.md".to_string(),
            document_reason: "docs_phrase".to_string(),
            document_rank: 1,
            document_raw_score: Some(1.0),
            document_has_exact_signal: false,
            document_has_docs_phrase_signal: true,
            document_docs_phrase_quality: Some(super::PhraseQuality::Generic),
            document_has_filename_signal: false,
            document_has_strict_lexical: false,
            lexical_strict_rank: None,
            lexical_broad_rank: Some(1),
            lexical_raw_score: Some(0.8),
            dense_rank: None,
            dense_raw_score: None,
            final_score: 1.0,
        };

        assert!(!has_strong_document_signal(&item));
    }

    #[test]
    fn implementation_lookup_query_classification_is_enabled_for_code_symbols() {
        assert!(is_implementation_lookup(&analyze_query(
            "POST /api/ask 现在返回什么协议？"
        )));
        assert!(is_implementation_lookup(&analyze_query(
            "ask_vault_structured"
        )));
        assert!(is_implementation_lookup(&analyze_query(
            "query_analysis_ms"
        )));
        assert!(is_implementation_lookup(&analyze_query("week8_report.md")));
        assert!(!is_implementation_lookup(&analyze_query(
            "POST /api/auth/oidc/login return?"
        )));
        assert!(!is_implementation_lookup(&analyze_query(
            "settings Advanced tab"
        )));
    }

    #[test]
    fn lookup_query_with_filename_and_lexical_support_is_not_rejected() {
        let analysis = analyze_query("week8_report.md");
        let evidence = vec![
            super::MergedEvidence {
                chunk: DocumentChunk {
                    file_path: PathBuf::from("docs/week8_report.md"),
                    content: "第八周周报摘要".to_string(),
                    chunk_index: 0,
                    heading_path: vec!["周报".to_string()],
                    block_kind: memori_parser::ChunkBlockKind::Paragraph,
                },
                relative_path: "docs/week8_report.md".to_string(),
                document_reason: "filename".to_string(),
                document_rank: 1,
                document_raw_score: Some(1.0),
                document_has_exact_signal: false,
                document_has_docs_phrase_signal: false,
                document_docs_phrase_quality: None,
                document_has_filename_signal: true,
                document_has_strict_lexical: false,
                lexical_strict_rank: Some(1),
                lexical_broad_rank: None,
                lexical_raw_score: Some(1.0),
                dense_rank: None,
                dense_raw_score: None,
                final_score: 1.0,
            },
            super::MergedEvidence {
                chunk: DocumentChunk {
                    file_path: PathBuf::from("docs/week8_report.md"),
                    content: "更多周报内容".to_string(),
                    chunk_index: 1,
                    heading_path: vec!["周报".to_string()],
                    block_kind: memori_parser::ChunkBlockKind::Paragraph,
                },
                relative_path: "docs/week8_report.md".to_string(),
                document_reason: "filename".to_string(),
                document_rank: 1,
                document_raw_score: Some(1.0),
                document_has_exact_signal: false,
                document_has_docs_phrase_signal: false,
                document_docs_phrase_quality: None,
                document_has_filename_signal: true,
                document_has_strict_lexical: false,
                lexical_strict_rank: Some(2),
                lexical_broad_rank: None,
                lexical_raw_score: Some(0.9),
                dense_rank: None,
                dense_raw_score: None,
                final_score: 0.9,
            },
        ];

        assert!(!should_refuse_for_insufficient_evidence(
            &analysis, &evidence
        ));
    }

    #[test]
    fn dense_only_long_query_is_rejected() {
        let analysis = analyze_query("请总结 week8_report.md 里的长跳转公式和实现细节");
        let evidence = vec![super::MergedEvidence {
            chunk: DocumentChunk {
                file_path: PathBuf::from("docs/week8_report.md"),
                content: "长跳转公式的实现细节".to_string(),
                chunk_index: 0,
                heading_path: vec!["周报".to_string()],
                block_kind: memori_parser::ChunkBlockKind::Paragraph,
            },
            relative_path: "docs/week8_report.md".to_string(),
            document_reason: "lexical_broad".to_string(),
            document_rank: 1,
            document_raw_score: Some(0.2),
            document_has_exact_signal: false,
            document_has_docs_phrase_signal: false,
            document_docs_phrase_quality: None,
            document_has_filename_signal: false,
            document_has_strict_lexical: false,
            lexical_strict_rank: None,
            lexical_broad_rank: None,
            lexical_raw_score: None,
            dense_rank: Some(1),
            dense_raw_score: Some(0.91),
            final_score: 1.0,
        }];

        assert!(should_refuse_for_insufficient_evidence(
            &analysis, &evidence
        ));
    }

    #[test]
    fn non_lookup_coverage_release_populates_metrics() {
        let analysis = analyze_query("新增的岗位是什么");
        let evidence = vec![super::MergedEvidence {
            chunk: DocumentChunk {
                file_path: PathBuf::from("docs/hiring.md"),
                content: "研发中心计划新增 12 个岗位，岗位包括后端与前端".to_string(),
                chunk_index: 0,
                heading_path: vec!["招聘计划".to_string()],
                block_kind: memori_parser::ChunkBlockKind::Paragraph,
            },
            relative_path: "docs/hiring.md".to_string(),
            document_reason: "lexical_broad".to_string(),
            document_rank: 1,
            document_raw_score: Some(0.8),
            document_has_exact_signal: false,
            document_has_docs_phrase_signal: false,
            document_docs_phrase_quality: None,
            document_has_filename_signal: false,
            document_has_strict_lexical: false,
            lexical_strict_rank: None,
            lexical_broad_rank: Some(1),
            lexical_raw_score: Some(0.6),
            dense_rank: None,
            dense_raw_score: None,
            final_score: 1.0,
        }];

        let mut metrics = RetrievalMetrics::default();
        let refused = apply_gating_metrics(&mut metrics, &analysis, &evidence);
        assert!(!refused);
        assert_eq!(metrics.gating_decision_reason, "coverage_release");
        assert!(metrics.top_doc_distinct_term_hits >= 2);
        assert!(metrics.top_doc_term_coverage >= 0.5);
    }

    #[test]
    fn lookup_like_high_coverage_lexical_evidence_is_not_rejected() {
        let analysis =
            analyze_query("物聯網 Internet of Things IoT UUID 通過網路傳輸數據的能力是什麼");
        assert!(analysis.flags.is_lookup_like);

        let evidence = vec![super::MergedEvidence {
            chunk: DocumentChunk {
                file_path: PathBuf::from("docs/iot.md"),
                content: "物聯網（英語：Internet of Things，簡稱 IoT）是一種計算設備、機械、數位機器相互關聯的系統，具備通用唯一辨識碼 UUID，並具有通過網路傳輸數據的能力。".to_string(),
                chunk_index: 0,
                heading_path: vec!["物联网".to_string()],
                block_kind: memori_parser::ChunkBlockKind::Paragraph,
            },
            relative_path: "docs/iot.md".to_string(),
            document_reason: "lexical_broad".to_string(),
            document_rank: 1,
            document_raw_score: Some(1.0),
            document_has_exact_signal: false,
            document_has_docs_phrase_signal: false,
            document_docs_phrase_quality: None,
            document_has_filename_signal: false,
            document_has_strict_lexical: false,
            lexical_strict_rank: None,
            lexical_broad_rank: Some(1),
            lexical_raw_score: Some(1.0),
            dense_rank: Some(1),
            dense_raw_score: Some(0.9),
            final_score: 1.0,
        }];

        let mut metrics = RetrievalMetrics::default();
        let refused = apply_gating_metrics(&mut metrics, &analysis, &evidence);
        assert!(!refused);
        assert_eq!(
            metrics.gating_decision_reason,
            "high_coverage_lexical_release"
        );
        assert!(metrics.top_doc_term_coverage >= 0.65);
    }

    #[test]
    fn missing_file_lookup_without_document_signal_is_rejected() {
        let analysis = analyze_query("请总结 week8_report.md 的内容");
        let evidence = vec![super::MergedEvidence {
            chunk: DocumentChunk {
                file_path: PathBuf::from("docs/other.md"),
                content: "无关内容".to_string(),
                chunk_index: 0,
                heading_path: vec!["其他".to_string()],
                block_kind: memori_parser::ChunkBlockKind::Paragraph,
            },
            relative_path: "docs/other.md".to_string(),
            document_reason: "lexical_broad".to_string(),
            document_rank: 1,
            document_raw_score: Some(0.1),
            document_has_exact_signal: false,
            document_has_docs_phrase_signal: false,
            document_docs_phrase_quality: None,
            document_has_filename_signal: false,
            document_has_strict_lexical: false,
            lexical_strict_rank: None,
            lexical_broad_rank: Some(1),
            lexical_raw_score: Some(0.2),
            dense_rank: None,
            dense_raw_score: None,
            final_score: 0.6,
        }];

        assert!(should_refuse_for_insufficient_evidence(
            &analysis, &evidence
        ));
    }

    #[test]
    fn explicit_insufficient_context_answer_is_not_treated_as_success() {
        assert!(super::engine::answer_indicates_insufficient_evidence(
            "当前上下文不足，缺少关于本周学习内容的直接记录。"
        ));
        assert!(super::engine::answer_indicates_insufficient_evidence(
            "There is insufficient context to answer this reliably."
        ));
    }

    #[test]
    fn grounded_answer_text_is_not_misclassified_as_insufficient() {
        assert!(!super::engine::answer_indicates_insufficient_evidence(
            "本周主要学习了 FORTIFY 绕过、CANNARY 检查与若干改进事项。"
        ));
    }

