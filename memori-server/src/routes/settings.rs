use crate::*;

pub(crate) async fn get_app_settings_handler(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<Json<AppSettingsDto>, ApiError> {
    let _ = require_session(&state, &headers, Role::Viewer).await?;
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
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(payload): Json<MemorySettingsDto>,
) -> Result<Json<AppSettingsDto>, ApiError> {
    let _ = require_session(&state, &headers, Role::Operator).await?;
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
    settings.retrieval_gating_profile =
        Some(normalize_gating_profile(&payload.retrieval_gating_profile));
    settings.generation_refusal_mode = Some(normalize_generation_refusal_mode(
        &payload.generation_refusal_mode,
    ));
    settings.gating_retry_on_refusal = Some(payload.gating_retry_on_refusal);
    apply_runtime_gating_settings(settings);
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

fn normalize_gating_profile(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "strict" => "strict",
        "answer_first" => "answer_first",
        _ => "balanced",
    }
    .to_string()
}

fn normalize_generation_refusal_mode(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "strict" => "strict",
        _ => "balanced",
    }
    .to_string()
}

fn apply_runtime_gating_settings(settings: &AppSettings) {
    // SAFETY: process-level runtime settings are the current config source.
    unsafe {
        std::env::set_var(
            memori_core::MEMORI_RETRIEVAL_GATING_PROFILE_ENV,
            settings
                .retrieval_gating_profile
                .clone()
                .unwrap_or_else(|| "balanced".to_string()),
        );
        std::env::set_var(
            memori_core::MEMORI_GENERATION_REFUSAL_MODE_ENV,
            settings
                .generation_refusal_mode
                .clone()
                .unwrap_or_else(|| "balanced".to_string()),
        );
        std::env::set_var(
            memori_core::MEMORI_GATING_RETRY_ON_REFUSAL_ENV,
            settings.gating_retry_on_refusal.unwrap_or(true).to_string(),
        );
    }
}

pub(crate) async fn set_watch_root_handler(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(payload): Json<SetWatchRootRequest>,
) -> Result<Json<AppSettingsDto>, ApiError> {
    let _ = require_session(&state, &headers, Role::Operator).await?;
    let trimmed = payload.path.trim();
    if trimmed.is_empty() {
        return Err(ApiError::bad_request("watch root path is empty"));
    }

    let watch_root = PathBuf::from(trimmed);
    if !watch_root.exists() {
        return Err(ApiError::bad_request(format!(
            "directory does not exist: {}",
            watch_root.display()
        )));
    }
    if !watch_root.is_dir() {
        return Err(ApiError::bad_request(format!(
            "path is not a directory: {}",
            watch_root.display()
        )));
    }

    let canonical = watch_root.canonicalize().map_err(|err| {
        ApiError::bad_request(format!("failed to canonicalize watch root: {err}"))
    })?;

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
    State(_state): State<ServerState>,
    Json(payload): Json<RankSettingsRequest>,
) -> Result<Json<RankSettingsResponse>, ApiError> {
    let query = payload.query.trim();
    if query.is_empty() || payload.candidates.is_empty() {
        return Ok(Json(RankSettingsResponse { keys: Vec::new() }));
    }

    let keys = rank_setting_keys(query, &payload.candidates, 3);
    Ok(Json(RankSettingsResponse { keys }))
}

pub(crate) fn rank_setting_keys(
    query: &str,
    candidates: &[SettingsSearchCandidate],
    limit: usize,
) -> Vec<String> {
    let normalized_query = normalize_rank_text(query);
    if normalized_query.is_empty() || candidates.is_empty() || limit == 0 {
        return Vec::new();
    }

    let query_terms = extract_rank_terms(&normalized_query);
    let mut scored = candidates
        .iter()
        .filter_map(|candidate| {
            let key = candidate.key.trim();
            if key.is_empty() {
                return None;
            }
            let text = candidate.text.trim();
            let score = score_setting_candidate(&normalized_query, &query_terms, key, text);
            (score > 0).then(|| (key.to_string(), score, text.len()))
        })
        .collect::<Vec<_>>();

    scored.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| a.2.cmp(&b.2))
            .then_with(|| a.0.cmp(&b.0))
    });
    scored.truncate(limit);
    scored.into_iter().map(|(key, _, _)| key).collect()
}

fn score_setting_candidate(query: &str, query_terms: &[String], key: &str, text: &str) -> i64 {
    let normalized_key = normalize_rank_text(key);
    let normalized_text = normalize_rank_text(text);
    let mut score = 0_i64;

    if normalized_key == query {
        score += 220;
    } else if normalized_key.starts_with(query) {
        score += 160;
    } else if normalized_key.contains(query) {
        score += 120;
    }

    if normalized_text.contains(query) {
        score += 90;
    }

    for term in query_terms {
        if term.is_empty() {
            continue;
        }
        if normalized_key == *term {
            score += 70;
        } else if normalized_key.starts_with(term) {
            score += 36;
        } else if normalized_key.contains(term) {
            score += 24;
        }

        if normalized_text.contains(term) {
            score += 16;
        }
    }

    if query_terms.len() > 1
        && query_terms
            .iter()
            .all(|term| normalized_text.contains(term))
    {
        score += 40;
    }

    score
}

fn normalize_rank_text(text: &str) -> String {
    text.trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| match ch {
            '-' | '_' | '/' | '\\' | '.' | ':' | ',' | '，' | '。' | '、' | '；' | ';' => ' ',
            _ => ch,
        })
        .collect::<String>()
}

fn extract_rank_terms(text: &str) -> Vec<String> {
    let mut terms = text
        .split_whitespace()
        .map(str::trim)
        .filter(|term| !term.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if !terms.iter().any(|term| term == text) {
        terms.push(text.to_string());
    }
    terms.sort();
    terms.dedup();
    terms
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(key: &str, text: &str) -> SettingsSearchCandidate {
        SettingsSearchCandidate {
            key: key.to_string(),
            text: text.to_string(),
        }
    }

    #[test]
    fn rank_setting_keys_prefers_exact_key_match() {
        let keys = rank_setting_keys(
            "mcp",
            &[
                candidate("models", "model settings"),
                candidate("mcp", "mcp integration"),
                candidate("memory", "memory settings"),
            ],
            3,
        );
        assert_eq!(keys.first().map(String::as_str), Some("mcp"));
    }

    #[test]
    fn rank_setting_keys_matches_descriptive_text() {
        let keys = rank_setting_keys(
            "context budget",
            &[
                candidate("models", "model provider and endpoint"),
                candidate("memory", "conversation memory and context budget controls"),
                candidate("indexing", "indexing mode and schedule"),
            ],
            3,
        );
        assert_eq!(keys.first().map(String::as_str), Some("memory"));
    }
}
