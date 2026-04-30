use super::*;

#[tauri::command]
pub(crate) async fn get_model_settings() -> Result<ModelSettingsDto, String> {
    let settings = load_app_settings()?;
    Ok(resolve_model_settings(&settings))
}

#[tauri::command]
pub(crate) async fn get_enterprise_policy() -> Result<EnterprisePolicyDto, String> {
    let settings = load_app_settings()?;
    Ok(resolve_enterprise_policy(&settings))
}

#[tauri::command]
pub(crate) async fn set_enterprise_policy(
    payload: EnterprisePolicyDto,
    state: State<'_, DesktopState>,
) -> Result<EnterprisePolicyDto, String> {
    info!(egress_mode = ?payload.egress_mode, endpoints = ?payload.allowed_model_endpoints.len(), models = ?payload.allowed_models.len(), "set enterprise policy");
    let mut settings = load_app_settings()?;
    settings.enterprise_egress_mode = Some(
        match payload.egress_mode {
            EgressMode::LocalOnly => "local_only",
            EgressMode::Allowlist => "allowlist",
        }
        .to_string(),
    );
    settings.enterprise_allowed_model_endpoints = Some(
        payload
            .allowed_model_endpoints
            .iter()
            .map(|item| normalize_policy_endpoint(item))
            .filter(|item| !item.is_empty())
            .collect(),
    );
    settings.enterprise_allowed_models = Some(
        payload
            .allowed_models
            .iter()
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect(),
    );
    save_app_settings(&settings)?;
    let watch_root = resolve_watch_root_from_settings(&settings)?;
    replace_engine(
        &state.engine,
        &state.init_error,
        watch_root,
        "settings_policy_update",
    )
    .await?;
    Ok(resolve_enterprise_policy(&settings))
}

#[tauri::command]
pub(crate) async fn set_model_settings(
    payload: ModelSettingsDto,
    state: State<'_, DesktopState>,
) -> Result<ModelSettingsDto, String> {
    info!(provider = %payload.active_provider, "set model settings");
    let mut settings = load_app_settings()?;
    let normalized = normalize_model_settings_payload(payload)?;
    let policy = resolve_enterprise_policy(&settings);
    validate_runtime_model_settings(
        &to_model_policy(&policy),
        &to_runtime_model_config(&resolve_active_runtime_settings(&normalized)),
    )
    .map_err(|violation| violation.message)?;
    settings.active_provider = Some(normalized.active_provider.clone());
    settings.local_chat_endpoint = Some(normalized.local_profile.chat_endpoint.clone());
    settings.local_graph_endpoint = Some(normalized.local_profile.graph_endpoint.clone());
    settings.local_embed_endpoint = Some(normalized.local_profile.embed_endpoint.clone());
    settings.local_endpoint = Some(normalized.local_profile.chat_endpoint.clone());
    settings.local_models_root = normalized.local_profile.models_root.clone();
    settings.local_llama_server_path = normalized.local_profile.llama_server_path.clone();
    settings.local_chat_model = Some(normalized.local_profile.chat_model.clone());
    settings.local_graph_model = Some(normalized.local_profile.graph_model.clone());
    settings.local_embed_model = Some(normalized.local_profile.embed_model.clone());
    settings.local_chat_model_path = normalized.local_profile.chat_model_path.clone();
    settings.local_graph_model_path = normalized.local_profile.graph_model_path.clone();
    settings.local_embed_model_path = normalized.local_profile.embed_model_path.clone();
    settings.remote_chat_endpoint = Some(normalized.remote_profile.chat_endpoint.clone());
    settings.remote_graph_endpoint = Some(normalized.remote_profile.graph_endpoint.clone());
    settings.remote_embed_endpoint = Some(normalized.remote_profile.embed_endpoint.clone());
    settings.remote_endpoint = Some(normalized.remote_profile.chat_endpoint.clone());
    settings.remote_api_key = normalized.remote_profile.api_key.clone();
    settings.remote_chat_model = Some(normalized.remote_profile.chat_model.clone());
    settings.remote_graph_model = Some(normalized.remote_profile.graph_model.clone());
    settings.remote_embed_model = Some(normalized.remote_profile.embed_model.clone());
    settings.local_chat_context_length = normalized.local_profile.chat_context_length;
    settings.local_graph_context_length = normalized.local_profile.graph_context_length;
    settings.local_embed_context_length = normalized.local_profile.embed_context_length;
    settings.local_chat_concurrency = normalized.local_profile.chat_concurrency;
    settings.local_graph_concurrency = normalized.local_profile.graph_concurrency;
    settings.local_embed_concurrency = normalized.local_profile.embed_concurrency;
    settings.local_performance_preset = normalized.local_profile.performance_preset.clone();
    settings.local_n_gpu_layers = normalized.local_profile.n_gpu_layers;
    settings.local_batch_size = normalized.local_profile.batch_size;
    settings.local_ubatch_size = normalized.local_profile.ubatch_size;
    settings.local_threads = normalized.local_profile.threads;
    settings.local_threads_batch = normalized.local_profile.threads_batch;
    settings.local_flash_attn = normalized.local_profile.flash_attn;
    settings.local_cache_type_k = normalized.local_profile.cache_type_k.clone();
    settings.local_cache_type_v = normalized.local_profile.cache_type_v.clone();
    settings.stop_local_models_on_exit = Some(normalized.stop_local_models_on_exit);
    settings.remote_chat_context_length = normalized.remote_profile.chat_context_length;
    settings.remote_graph_context_length = normalized.remote_profile.graph_context_length;
    settings.remote_embed_context_length = normalized.remote_profile.embed_context_length;
    settings.remote_chat_concurrency = normalized.remote_profile.chat_concurrency;
    settings.remote_graph_concurrency = normalized.remote_profile.graph_concurrency;
    settings.remote_embed_concurrency = normalized.remote_profile.embed_concurrency;
    save_app_settings(&settings)?;

    let watch_root = resolve_watch_root_from_settings(&settings)?;
    replace_engine(
        &state.engine,
        &state.init_error,
        watch_root,
        "settings_model_update",
    )
    .await?;

    Ok(normalized)
}

#[tauri::command]
#[allow(non_snake_case)]
pub(crate) async fn list_provider_models(
    provider: String,
    chatEndpoint: String,
    graphEndpoint: String,
    embedEndpoint: String,
    apiKey: Option<String>,
    modelsRoot: Option<String>,
) -> Result<ProviderModelsDto, String> {
    info!(provider = %provider, "list provider models");
    let settings = load_app_settings()?;
    let policy = resolve_enterprise_policy(&settings);
    let provider = ModelProvider::from_value(&provider);
    let chat_endpoint = normalize_endpoint(provider, &chatEndpoint);
    let graph_endpoint = normalize_endpoint(provider, &graphEndpoint);
    let embed_endpoint = normalize_endpoint(provider, &embedEndpoint);
    let api_key = normalize_optional_text(apiKey);
    let models_root = normalize_optional_text(modelsRoot);
    for endpoint in [&chat_endpoint, &graph_endpoint, &embed_endpoint] {
        validate_provider_request(&to_model_policy(&policy), provider, endpoint, &[])
            .map_err(|violation| violation.message)?;
    }
    let (dto, _errors) = fetch_models_all_endpoints(
        provider,
        &chat_endpoint,
        &graph_endpoint,
        &embed_endpoint,
        api_key.as_deref(),
        models_root.as_deref(),
    )
    .await
    .map_err(|err| format!("{}: {}", err.code, err.message))?;
    Ok(dto)
}

#[tauri::command]
#[allow(non_snake_case)]
pub(crate) async fn probe_model_provider(
    provider: String,
    chatEndpoint: String,
    graphEndpoint: String,
    embedEndpoint: String,
    apiKey: Option<String>,
    modelsRoot: Option<String>,
) -> Result<ModelAvailabilityDto, String> {
    info!(provider = %provider, "probe model provider");
    let settings = load_app_settings()?;
    let policy = resolve_enterprise_policy(&settings);
    let provider = ModelProvider::from_value(&provider);
    let chat_endpoint = normalize_endpoint(provider, &chatEndpoint);
    let graph_endpoint = normalize_endpoint(provider, &graphEndpoint);
    let embed_endpoint = normalize_endpoint(provider, &embedEndpoint);
    let api_key = normalize_optional_text(apiKey);
    let models_root = normalize_optional_text(modelsRoot);
    for endpoint in [&chat_endpoint, &graph_endpoint, &embed_endpoint] {
        validate_provider_request(&to_model_policy(&policy), provider, endpoint, &[])
            .map_err(|violation| violation.message)?;
    }
    let (models, errors) = fetch_models_all_endpoints(
        provider,
        &chat_endpoint,
        &graph_endpoint,
        &embed_endpoint,
        api_key.as_deref(),
        models_root.as_deref(),
    )
    .await
    .map_err(|err| format!("{}: {}", err.code, err.message))?;

    let merged = models.merged;
    let mut missing_roles = Vec::new();

    let (chat_model, graph_model, embed_model) = match provider {
        ModelProvider::LlamaCppLocal => (
            settings.local_chat_model.as_deref().unwrap_or(""),
            settings.local_graph_model.as_deref().unwrap_or(""),
            settings.local_embed_model.as_deref().unwrap_or(""),
        ),
        ModelProvider::OpenAiCompatible => (
            settings.remote_chat_model.as_deref().unwrap_or(""),
            settings.remote_graph_model.as_deref().unwrap_or(""),
            settings.remote_embed_model.as_deref().unwrap_or(""),
        ),
    };

    if !chat_model.is_empty() && !model_exists(&merged, chat_model) {
        missing_roles.push("chat".to_string());
    }
    if !graph_model.is_empty() && !model_exists(&merged, graph_model) {
        missing_roles.push("graph".to_string());
    }
    if !embed_model.is_empty() && !model_exists(&merged, embed_model) {
        missing_roles.push("embed".to_string());
    }

    let reachable = errors.is_empty();

    Ok(ModelAvailabilityDto {
        configured: true,
        reachable,
        models: merged,
        missing_roles,
        errors: errors
            .into_iter()
            .map(|err| ModelErrorItem {
                code: err.code,
                message: err.message,
            })
            .collect(),
        checked_provider: Some(provider_to_string(provider)),
        status_code: None,
        status_message: None,
    })
}

#[tauri::command]
pub(crate) async fn validate_model_setup() -> Result<ModelAvailabilityDto, String> {
    let settings = load_app_settings()?;
    let Some(active) = resolve_configured_active_runtime_settings(&settings) else {
        let checked_provider = resolve_explicit_provider(&settings).map(provider_to_string);
        return Ok(ModelAvailabilityDto {
            configured: false,
            reachable: false,
            models: Vec::new(),
            missing_roles: Vec::new(),
            errors: vec![ModelErrorItem {
                code: MODEL_NOT_CONFIGURED_CODE.to_string(),
                message: MODEL_NOT_CONFIGURED_MESSAGE.to_string(),
            }],
            checked_provider,
            status_code: Some(MODEL_NOT_CONFIGURED_CODE.to_string()),
            status_message: Some(MODEL_NOT_CONFIGURED_MESSAGE.to_string()),
        });
    };
    let policy = resolve_enterprise_policy(&settings);
    validate_runtime_model_settings(&to_model_policy(&policy), &to_runtime_model_config(&active))
        .map_err(|violation| violation.message)?;
    let provider = active.provider;
    let (models, endpoint_errors) = fetch_models_all_endpoints(
        provider,
        &active.chat_endpoint,
        &active.graph_endpoint,
        &active.embed_endpoint,
        active.api_key.as_deref(),
        active.models_root.as_deref(),
    )
    .await
    .map_err(|err| format!("{}: {}", err.code, err.message))?;

    let merged = models.merged;
    let mut missing_roles = Vec::new();
    let mut errors: Vec<ModelErrorItem> = endpoint_errors
        .into_iter()
        .map(|err| ModelErrorItem {
            code: err.code,
            message: err.message,
        })
        .collect();

    if !model_exists(&merged, &active.chat_model) {
        missing_roles.push("chat".to_string());
    }
    if !model_exists(&merged, &active.graph_model) {
        missing_roles.push("graph".to_string());
    }
    if !model_exists(&merged, &active.embed_model) {
        missing_roles.push("embed".to_string());
    }
    if provider == ModelProvider::OpenAiCompatible
        && !missing_roles.iter().any(|role| role == "embed")
        && let Err(err) = probe_openai_compatible_embedding(
            &active.embed_endpoint,
            active.api_key.as_deref(),
            &active.embed_model,
        )
        .await
    {
        missing_roles.push("embed".to_string());
        errors.push(ModelErrorItem {
            code: err.code,
            message: err.message,
        });
    }

    Ok(ModelAvailabilityDto {
        configured: true,
        reachable: errors.is_empty(),
        models: merged,
        missing_roles,
        errors,
        checked_provider: Some(provider_to_string(provider)),
        status_code: None,
        status_message: None,
    })
}

#[tauri::command]
pub(crate) async fn get_local_model_runtime_status(
    state: State<'_, DesktopState>,
) -> Result<LocalModelRuntimeStatusesDto, String> {
    let settings = load_app_settings()?;
    let profile = resolve_model_settings(&settings).local_profile;
    local_runtime_statuses(&state, &profile).await
}

#[tauri::command]
pub(crate) async fn start_local_model(
    role: String,
    state: State<'_, DesktopState>,
) -> Result<LocalModelRuntimeStatusesDto, String> {
    let role = normalize_local_model_role(&role)?;
    let settings = load_app_settings()?;
    let profile = resolve_model_settings(&settings).local_profile;
    start_local_model_role(&role, &profile, &state).await?;
    if role == "embed" {
        resume_engine_indexing_if_ready(&state).await;
    }
    local_runtime_statuses(&state, &profile).await
}

#[tauri::command]
pub(crate) async fn stop_local_model(
    role: String,
    state: State<'_, DesktopState>,
) -> Result<LocalModelRuntimeStatusesDto, String> {
    let role = normalize_local_model_role(&role)?;
    stop_local_model_role(&role, &state).await?;
    let settings = load_app_settings()?;
    let profile = resolve_model_settings(&settings).local_profile;
    local_runtime_statuses(&state, &profile).await
}

#[tauri::command]
pub(crate) async fn restart_local_model(
    role: String,
    state: State<'_, DesktopState>,
) -> Result<LocalModelRuntimeStatusesDto, String> {
    let role = normalize_local_model_role(&role)?;
    let settings = load_app_settings()?;
    let profile = resolve_model_settings(&settings).local_profile;
    stop_local_model_role(&role, &state).await?;
    start_local_model_role(&role, &profile, &state).await?;
    if role == "embed" {
        resume_engine_indexing_if_ready(&state).await;
    }
    local_runtime_statuses(&state, &profile).await
}

async fn resume_engine_indexing_if_ready(state: &State<'_, DesktopState>) {
    let engine_guard = state.engine.lock().await;
    if let Some(engine) = engine_guard.as_ref() {
        engine.resume_indexing().await;
    }
}

#[tauri::command]
pub(crate) async fn pull_model(
    model: String,
    provider: String,
    endpoint: String,
    api_key: Option<String>,
) -> Result<ModelAvailabilityDto, String> {
    info!(model = %model, provider = %provider, "pull model requested");
    let model = model.trim().to_string();
    if model.is_empty() {
        return Err("model name cannot be empty".to_string());
    }
    let provider = ModelProvider::from_value(&provider);
    let endpoint = normalize_endpoint(provider, &endpoint);
    let settings = load_app_settings()?;
    let policy = resolve_enterprise_policy(&settings);
    validate_provider_request(&to_model_policy(&policy), provider, &endpoint, &[])
        .map_err(|violation| violation.message)?;
    let api_key = normalize_optional_text(api_key);
    pull_llama_cpp_model(&endpoint, &model, api_key.as_deref()).await?;
    validate_model_setup().await
}

#[tauri::command]
pub(crate) async fn set_local_models_root(path: String) -> Result<ModelSettingsDto, String> {
    info!(path = %path, "set local models root");
    let mut settings = load_app_settings()?;
    let root = normalize_optional_text(Some(path));
    if let Some(root_path) = root.as_deref() {
        let path = PathBuf::from(root_path);
        if !path.exists() {
            return Err(format!("models root does not exist: {}", path.display()));
        }
        if !path.is_dir() {
            return Err(format!("path is not a directory: {}", path.display()));
        }
        settings.local_models_root = Some(
            path.canonicalize()
                .unwrap_or(path)
                .to_string_lossy()
                .to_string(),
        );
    } else {
        settings.local_models_root = None;
    }
    save_app_settings(&settings)?;
    Ok(resolve_model_settings(&settings))
}

#[tauri::command]
pub(crate) async fn scan_local_model_files(root: Option<String>) -> Result<Vec<String>, String> {
    info!(root = ?root, "scan local model files");
    let root = normalize_optional_text(root);
    if let Some(root) = root {
        return scan_local_model_files_from_root(&PathBuf::from(root));
    }
    let settings = load_app_settings()?;
    let resolved = resolve_model_settings(&settings);
    let Some(root) = resolved.local_profile.models_root else {
        return Ok(Vec::new());
    };
    scan_local_model_files_from_root(&PathBuf::from(root))
}
