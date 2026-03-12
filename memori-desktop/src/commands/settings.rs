use super::*;

#[tauri::command]
pub(crate) async fn get_app_settings() -> Result<AppSettingsDto, String> {
    let settings = load_app_settings()?;
    let watch_root = resolve_watch_root_from_settings(&settings)?;
    let indexing = resolve_indexing_config(&settings);
    Ok(AppSettingsDto {
        watch_root: watch_root.to_string_lossy().to_string(),
        language: settings.language,
        indexing_mode: indexing.mode.as_str().to_string(),
        resource_budget: indexing.resource_budget.as_str().to_string(),
        schedule_start: indexing.schedule_window.as_ref().map(|w| w.start.clone()),
        schedule_end: indexing.schedule_window.as_ref().map(|w| w.end.clone()),
    })
}

#[tauri::command]
pub(crate) async fn rank_settings_query(
    query: String,
    candidates: Vec<SettingsSearchCandidate>,
    lang: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<Vec<String>, String> {
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
