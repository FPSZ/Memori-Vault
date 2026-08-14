use crate::*;

pub(crate) async fn ask_handler(
    State(state): State<ServerState>,
    Extension(request_id): Extension<RequestId>,
    headers: HeaderMap,
    Json(payload): Json<AskRequest>,
) -> Result<Json<AskResponseStructured>, ApiError> {
    state.metrics.total_requests.fetch_add(1, Ordering::Relaxed);
    state.metrics.ask_requests.fetch_add(1, Ordering::Relaxed);
    let ask_started_at = std::time::Instant::now();

    let query = payload.query.trim().to_string();
    if query.is_empty() {
        state
            .metrics
            .failed_requests
            .fetch_add(1, Ordering::Relaxed);
        state.metrics.ask_failed.fetch_add(1, Ordering::Relaxed);
        return Ok(Json(AskResponseStructured {
            status: AskStatus::InsufficientEvidence,
            answer: String::new(),
            question: query,
            scope_paths: Vec::new(),
            citations: Vec::new(),
            evidence: Vec::new(),
            metrics: Default::default(),
            answer_source_mix: memori_core::AnswerSourceMix::Insufficient,
            memory_context: Vec::new(),
            source_groups: Vec::new(),
            failure_class: memori_core::FailureClass::RecallMiss,
            context_budget_report: memori_core::ContextBudgetReport::default(),
        }));
    }

    let session = require_session(&state, &headers, Role::Viewer).await?;
    let actor = session.subject;
    let policy = resolve_enterprise_policy(&load_app_settings().map_err(ApiError::internal)?);
    if let Err(violation) = validate_runtime_model_settings(
        &to_model_policy(&policy),
        &resolve_runtime_model_config_from_env(),
    ) {
        append_policy_violation_audit(
            &state,
            actor.clone(),
            "ask",
            None,
            None,
            &[],
            &violation.message,
        )
        .await;
        return Err(ApiError::forbidden(violation.message));
    }

    let init_error_message = state.init_error.lock().await.clone();
    let engine_guard = state.engine.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        state
            .metrics
            .failed_requests
            .fetch_add(1, Ordering::Relaxed);
        state.metrics.ask_failed.fetch_add(1, Ordering::Relaxed);
        if let Some(message) = init_error_message {
            return Err(ApiError::internal(format!("引擎初始化失败: {message}")));
        }
        return Err(ApiError::internal("引擎尚在初始化中，请稍后重试。"));
    };

    let top_k = normalize_top_k(payload.top_k);
    let scope_paths = normalize_scope_paths(payload.scope_paths);
    let scope_refs = if scope_paths.is_empty() {
        None
    } else {
        Some(scope_paths.as_slice())
    };

    let response = engine
        .ask_structured(&query, payload.lang.as_deref(), scope_refs, payload.top_k)
        .await
        .map_err(|err| {
            state
                .metrics
                .failed_requests
                .fetch_add(1, Ordering::Relaxed);
            state.metrics.ask_failed.fetch_add(1, Ordering::Relaxed);
            warn!(
                request_id = %request_id.0,
                error = %err,
                elapsed_ms = ask_started_at.elapsed().as_millis() as u64,
                "ask failed"
            );
            map_engine_api_error(err)
        })?;

    let elapsed_ms = ask_started_at.elapsed().as_millis() as u64;
    state
        .metrics
        .ask_latency_total_ms
        .fetch_add(elapsed_ms, Ordering::Relaxed);
    // 完成事件：handler 在 http_request span 内执行，JSON 日志自带 span 字段；
    // 这里显式带上 request_id 让控制台输出同样可按请求关联。默认 info 级别可见，
    // 各阶段耗时来自检索链打点，与响应 metrics 一致。
    info!(
        request_id = %request_id.0,
        status = ?response.status,
        doc_recall_ms = response.metrics.doc_recall_ms,
        chunk_dense_ms = response.metrics.chunk_dense_ms,
        chunk_lexical_ms = response.metrics.chunk_lexical_ms,
        merge_ms = response.metrics.merge_ms,
        rerank_ms = response.metrics.rerank_ms,
        answer_ms = response.metrics.answer_ms,
        evidence_count = response.metrics.final_evidence_count,
        elapsed_ms,
        "ask completed"
    );
    append_audit_event(
        &state,
        AuditEventDto {
            actor,
            action: "query.ask".to_string(),
            resource: "vault".to_string(),
            timestamp: unix_now_secs(),
            result: "ok".to_string(),
            metadata: serde_json::json!({
                "request_id": request_id.0,
                "top_k": top_k,
                "scope_count": scope_paths.len(),
                "status": response.status,
                "doc_candidate_count": response.metrics.doc_candidate_count,
                "final_evidence_count": response.metrics.final_evidence_count,
                "elapsed_ms": elapsed_ms
            }),
        },
    )
    .await;

    Ok(Json(response))
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AskLegacyResponse {
    answer: String,
}

pub(crate) async fn ask_legacy_handler(
    State(state): State<ServerState>,
    Extension(request_id): Extension<RequestId>,
    headers: HeaderMap,
    Json(payload): Json<AskRequest>,
) -> Result<Json<AskLegacyResponse>, ApiError> {
    let response = ask_handler(State(state), Extension(request_id), headers, Json(payload)).await?;
    Ok(Json(AskLegacyResponse {
        answer: format_legacy_answer(&response.0),
    }))
}
