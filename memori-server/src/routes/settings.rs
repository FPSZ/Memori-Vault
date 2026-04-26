use crate::*;

pub(crate) async fn get_app_settings_handler() -> Result<Json<AppSettingsDto>, ApiError> {
    let settings = load_app_settings().map_err(ApiError::internal)?;
    let watch_root = resolve_watch_root_from_settings(&settings).map_err(ApiError::internal)?;
    let indexing = resolve_indexing_config(&settings);
    Ok(Json(AppSettingsDto::from_settings(
        settings,
        watch_root.to_string_lossy().to_string(),
        indexing,
    )))
}

pub(crate) async fn set_memory_settings_handler(
    Json(payload): Json<MemorySettingsDto>,
) -> Result<Json<AppSettingsDto>, ApiError> {
    let mut settings = load_app_settings().map_err(ApiError::internal)?;
    apply_memory_settings(&mut settings, payload);
    save_app_settings(&settings).map_err(ApiError::internal)?;
    let watch_root = resolve_watch_root_from_settings(&settings).map_err(ApiError::internal)?;
    let indexing = resolve_indexing_config(&settings);
    Ok(Json(AppSettingsDto::from_settings(
        settings,
        watch_root.to_string_lossy().to_string(),
        indexing,
    )))
}

fn apply_memory_settings(settings: &mut AppSettings, payload: MemorySettingsDto) {
    settings.conversation_memory_enabled = Some(payload.conversation_memory_enabled);
    settings.auto_memory_write = Some(normalize_auto_memory_write(&payload.auto_memory_write));
    settings.memory_write_requires_source = Some(payload.memory_write_requires_source);
    settings.memory_markdown_export_enabled = Some(payload.memory_markdown_export_enabled);
    settings.default_context_budget = Some(normalize_context_budget(
        &payload.default_context_budget,
        "16k",
    ));
    settings.complex_context_budget = Some(normalize_context_budget(
        &payload.complex_context_budget,
        "32k",
    ));
    settings.graph_ranking_enabled = Some(payload.graph_ranking_enabled);
}

fn normalize_auto_memory_write(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "off" => "off",
        "auto_low_risk" => "auto_low_risk",
        _ => "suggest",
    }
    .to_string()
}

fn normalize_context_budget(value: &str, fallback: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "8k" => "8k",
        "16k" => "16k",
        "32k" => "32k",
        "64k" => "64k",
        _ => fallback,
    }
    .to_string()
}

pub(crate) async fn set_watch_root_handler(
    State(state): State<ServerState>,
    Json(payload): Json<SetWatchRootRequest>,
) -> Result<Json<AppSettingsDto>, ApiError> {
    let trimmed = payload.path.trim();
    if trimmed.is_empty() {
        return Err(ApiError::bad_request("目录路径为空，无法保存。"));
    }

    let watch_root = PathBuf::from(trimmed);
    if !watch_root.exists() {
        return Err(ApiError::bad_request(format!(
            "目录不存在: {}",
            watch_root.display()
        )));
    }
    if !watch_root.is_dir() {
        return Err(ApiError::bad_request(format!(
            "路径不是目录: {}",
            watch_root.display()
        )));
    }

    let canonical = watch_root
        .canonicalize()
        .map_err(|err| ApiError::bad_request(format!("规范化目录失败: {err}")))?;

    let mut settings = load_app_settings().map_err(ApiError::internal)?;
    settings.watch_root = Some(canonical.to_string_lossy().to_string());
    save_app_settings(&settings).map_err(ApiError::internal)?;

    replace_engine(
        &state.engine,
        &state.init_error,
        canonical.clone(),
        "settings_watch_root_update",
    )
    .await
    .map_err(ApiError::internal)?;

    let indexing = resolve_indexing_config(&settings);
    Ok(Json(AppSettingsDto::from_settings(
        settings,
        canonical.to_string_lossy().to_string(),
        indexing,
    )))
}

pub(crate) async fn rank_settings_query_handler(
    State(state): State<ServerState>,
    Json(payload): Json<RankSettingsRequest>,
) -> Result<Json<RankSettingsResponse>, ApiError> {
    let query = payload.query.trim();
    if query.is_empty() || payload.candidates.is_empty() {
        return Ok(Json(RankSettingsResponse { keys: Vec::new() }));
    }

    let init_error_message = state.init_error.lock().await.clone();
    let engine_guard = state.engine.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_message {
            return Err(ApiError::internal(format!("引擎初始化失败: {message}")));
        }
        return Err(ApiError::internal("引擎尚在初始化中，请稍后重试。"));
    };

    let mut candidate_lines = Vec::with_capacity(payload.candidates.len());
    for item in &payload.candidates {
        candidate_lines.push(format!("{} => {}", item.key.trim(), item.text.trim()));
    }

    let prompt = match normalize_language(payload.lang.as_deref()) {
        Some("zh-CN") => format!(
            "你是设置检索助手。用户搜索词：{query}\n候选设置项：\n{}\n\n请仅返回 JSON 数组，内容为最匹配的 key，最多 3 个。示例：[\"basic\",\"models\"]。\n禁止输出解释文字。",
            candidate_lines.join("\n")
        ),
        _ => format!(
            "You are a settings retrieval assistant.\nQuery: {query}\nCandidates:\n{}\n\nReturn only a JSON array of best-matching keys, max 3. Example: [\"basic\",\"models\"].\nDo not output explanations.",
            candidate_lines.join("\n")
        ),
    };

    let answer = engine
        .generate_answer(&prompt, "", "")
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;

    let candidate_keys: HashSet<String> = payload
        .candidates
        .iter()
        .map(|c| c.key.trim().to_string())
        .collect();

    if let Ok(parsed) = serde_json::from_str::<Vec<String>>(&answer) {
        let matched = parsed
            .into_iter()
            .filter(|key| candidate_keys.contains(key.trim()))
            .collect::<Vec<_>>();
        if !matched.is_empty() {
            return Ok(Json(RankSettingsResponse { keys: matched }));
        }
    }

    if let (Some(start), Some(end)) = (answer.find('['), answer.rfind(']'))
        && start < end
    {
        let json_slice = &answer[start..=end];
        if let Ok(parsed) = serde_json::from_str::<Vec<String>>(json_slice) {
            let matched = parsed
                .into_iter()
                .filter(|key| candidate_keys.contains(key.trim()))
                .collect::<Vec<_>>();
            if !matched.is_empty() {
                return Ok(Json(RankSettingsResponse { keys: matched }));
            }
        }
    }

    let lower_answer = answer.to_ascii_lowercase();
    let fallback = payload
        .candidates
        .iter()
        .filter_map(|candidate| {
            let key = candidate.key.trim().to_string();
            if key.is_empty() {
                return None;
            }
            if lower_answer.contains(&key.to_ascii_lowercase()) {
                Some(key)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    Ok(Json(RankSettingsResponse { keys: fallback }))
}
