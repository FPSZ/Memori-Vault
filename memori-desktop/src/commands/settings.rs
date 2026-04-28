use super::*;

#[tauri::command]
pub(crate) async fn get_app_settings() -> Result<AppSettingsDto, String> {
    info!("[用户操作] 获取应用设置");
    let settings = load_app_settings()?;
    let watch_root = resolve_watch_root_from_settings(&settings)?;
    let indexing = resolve_indexing_config(&settings);
    Ok(AppSettingsDto::from_settings(
        settings,
        watch_root.to_string_lossy().to_string(),
        indexing,
    ))
}

#[tauri::command]
pub(crate) async fn set_memory_settings(
    payload: MemorySettingsDto,
) -> Result<AppSettingsDto, String> {
    info!("[用户操作] 修改分层记忆与上下文设置");
    let mut settings = load_app_settings()?;
    apply_memory_settings(&mut settings, payload);
    save_app_settings(&settings)?;
    let watch_root = resolve_watch_root_from_settings(&settings)?;
    let indexing = resolve_indexing_config(&settings);
    Ok(AppSettingsDto::from_settings(
        settings,
        watch_root.to_string_lossy().to_string(),
        indexing,
    ))
}

fn apply_memory_settings(settings: &mut AppSettings, payload: MemorySettingsDto) {
    settings.conversation_memory_enabled = Some(payload.conversation_memory_enabled);
    settings.auto_memory_write = Some(normalize_auto_memory_write(&payload.auto_memory_write));
    settings.memory_write_requires_source = Some(payload.memory_write_requires_source);
    // Markdown export is still a planned architecture capability. Keep the
    // persisted flag false so the UI/API cannot imply that export is active.
    settings.memory_markdown_export_enabled = Some(false);
    settings.default_context_budget = Some(normalize_context_budget(
        &payload.default_context_budget,
        "16k",
    ));
    settings.complex_context_budget = Some(normalize_context_budget(
        &payload.complex_context_budget,
        "32k",
    ));
    // ADR-003/P1 keeps graph as explanation context only; it must not affect
    // the main retrieval ranking until an explicit ranking experiment ships.
    settings.graph_ranking_enabled = Some(false);
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

#[tauri::command]
pub(crate) async fn rank_settings_query(
    query: String,
    candidates: Vec<SettingsSearchCandidate>,
    lang: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<Vec<String>, String> {
    info!(query = %query, "[用户操作] 设置搜索");
    let query = query.trim();
    if query.is_empty() || candidates.is_empty() {
        return Ok(Vec::new());
    }

    let engine_guard = state.engine.lock().await;
    let init_error_guard = state.init_error.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_guard.as_ref() {
            return Err(format!("引擎初始化失败: {message}"));
        }
        return Err("引擎尚在初始化中，请稍后重试。".to_string());
    };

    let mut candidate_lines = Vec::with_capacity(candidates.len());
    for item in &candidates {
        candidate_lines.push(format!("{} => {}", item.key.trim(), item.text.trim()));
    }

    let prompt = match normalize_language(lang.as_deref()) {
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
        .map_err(|err| err.to_string())?;

    let candidate_keys: std::collections::HashSet<String> = candidates
        .iter()
        .map(|c| c.key.trim().to_string())
        .collect();

    if let Ok(parsed) = serde_json::from_str::<Vec<String>>(&answer) {
        let matched = parsed
            .into_iter()
            .filter(|key| candidate_keys.contains(key.trim()))
            .collect::<Vec<_>>();
        if !matched.is_empty() {
            return Ok(matched);
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
                return Ok(matched);
            }
        }
    }

    let lower_answer = answer.to_ascii_lowercase();
    let fallback = candidates
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

    Ok(fallback)
}
