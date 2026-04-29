use crate::*;

pub(crate) async fn get_model_settings_handler() -> Result<Json<ModelSettingsDto>, ApiError> {
    let settings = load_app_settings().map_err(ApiError::internal)?;
    Ok(Json(resolve_model_settings(&settings)))
}

pub(crate) async fn set_model_settings_handler(
    State(state): State<ServerState>,
    Json(payload): Json<ModelSettingsDto>,
) -> Result<Json<ModelSettingsDto>, ApiError> {
    let mut settings = load_app_settings().map_err(ApiError::internal)?;
    let normalized = normalize_model_settings_payload(payload).map_err(ApiError::bad_request)?;
    let policy = resolve_enterprise_policy(&settings);
    let active_runtime = resolve_active_runtime_settings(&normalized);
    if let Err(violation) = validate_runtime_model_settings(
        &to_model_policy(&policy),
        &to_runtime_model_config(&active_runtime),
    ) {
        append_policy_violation_audit(
            &state,
            "anonymous".to_string(),
            "set_model_settings",
            Some(active_runtime.provider),
            Some(&active_runtime.chat_endpoint),
            &[
                active_runtime.chat_model.clone(),
                active_runtime.graph_model.clone(),
                active_runtime.embed_model.clone(),
            ],
            &violation.message,
        )
        .await;
        return Err(ApiError::forbidden(violation.message));
    }
    settings.active_provider = Some(normalized.active_provider.clone());
    settings.local_endpoint = Some(normalized.local_profile.endpoint.clone());
    settings.local_models_root = normalized.local_profile.models_root.clone();
    settings.local_chat_model = Some(normalized.local_profile.chat_model.clone());
    settings.local_graph_model = Some(normalized.local_profile.graph_model.clone());
    settings.local_embed_model = Some(normalized.local_profile.embed_model.clone());
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
    settings.remote_endpoint = Some(normalized.remote_profile.endpoint.clone());
    settings.remote_api_key = normalized.remote_profile.api_key.clone();
    settings.remote_chat_model = Some(normalized.remote_profile.chat_model.clone());
    settings.remote_graph_model = Some(normalized.remote_profile.graph_model.clone());
    settings.remote_embed_model = Some(normalized.remote_profile.embed_model.clone());
    settings.remote_chat_context_length = normalized.remote_profile.chat_context_length;
    settings.remote_graph_context_length = normalized.remote_profile.graph_context_length;
    settings.remote_embed_context_length = normalized.remote_profile.embed_context_length;
    settings.remote_chat_concurrency = normalized.remote_profile.chat_concurrency;
    settings.remote_graph_concurrency = normalized.remote_profile.graph_concurrency;
    settings.remote_embed_concurrency = normalized.remote_profile.embed_concurrency;
    save_app_settings(&settings).map_err(ApiError::internal)?;
    apply_model_settings_to_env(resolve_active_runtime_settings(&normalized));

    let watch_root = resolve_watch_root_from_settings(&settings).map_err(ApiError::internal)?;
    replace_engine(
        &state.engine,
        &state.init_error,
        watch_root,
        "settings_model_update",
    )
    .await
    .map_err(ApiError::internal)?;

    Ok(Json(normalized))
}

pub(crate) async fn validate_model_setup_handler(
    State(state): State<ServerState>,
) -> Result<Json<ModelAvailabilityDto>, ApiError> {
    let settings = load_app_settings().map_err(ApiError::internal)?;
    let model_settings = resolve_model_settings(&settings);
    let active = resolve_active_runtime_settings(&model_settings);
    let policy = resolve_enterprise_policy(&settings);
    if let Err(violation) = validate_runtime_model_settings(
        &to_model_policy(&policy),
        &to_runtime_model_config(&active),
    ) {
        append_policy_violation_audit(
            &state,
            "anonymous".to_string(),
            "validate_model_setup",
            Some(active.provider),
            Some(&active.chat_endpoint),
            &[
                active.chat_model.clone(),
                active.graph_model.clone(),
                active.embed_model.clone(),
            ],
            &violation.message,
        )
        .await;
        return Err(ApiError::forbidden(violation.message));
    }
    let provider = active.provider;
    let models = fetch_provider_models(
        provider,
        &active.chat_endpoint,
        active.api_key.as_deref(),
        active.models_root.as_deref(),
    )
    .await;

    match models {
        Ok(models) => {
            let merged = models.merged;
            let mut missing_roles = Vec::new();
            if !model_exists(&merged, &active.chat_model) {
                missing_roles.push("chat".to_string());
            }
            if !model_exists(&merged, &active.graph_model) {
                missing_roles.push("graph".to_string());
            }
            if !model_exists(&merged, &active.embed_model) {
                missing_roles.push("embed".to_string());
            }
            Ok(Json(ModelAvailabilityDto {
                reachable: true,
                models: merged,
                missing_roles,
                errors: Vec::new(),
                checked_provider: Some(provider_to_string(provider)),
            }))
        }
        Err(err) => Ok(Json(ModelAvailabilityDto {
            reachable: false,
            models: Vec::new(),
            missing_roles: vec!["chat".to_string(), "graph".to_string(), "embed".to_string()],
            errors: vec![ModelErrorItem {
                code: err.code,
                message: err.message,
            }],
            checked_provider: Some(provider_to_string(provider)),
        })),
    }
}

pub(crate) async fn list_provider_models_handler(
    State(state): State<ServerState>,
    Json(payload): Json<ListProviderModelsRequest>,
) -> Result<Json<ProviderModelsDto>, ApiError> {
    let settings = load_app_settings().map_err(ApiError::internal)?;
    let policy = resolve_enterprise_policy(&settings);
    let provider = ModelProvider::from_value(&payload.provider);
    let endpoint = normalize_endpoint(provider, &payload.endpoint);
    let api_key = normalize_optional_text(payload.api_key);
    let models_root = normalize_optional_text(payload.models_root);
    if let Err(violation) =
        validate_provider_request(&to_model_policy(&policy), provider, &endpoint, &[])
    {
        append_policy_violation_audit(
            &state,
            "anonymous".to_string(),
            "list_provider_models",
            Some(provider),
            Some(&endpoint),
            &[],
            &violation.message,
        )
        .await;
        return Err(ApiError::forbidden(violation.message));
    }
    if provider == ModelProvider::LlamaCppLocal {
        let from_folder = models_root
            .as_deref()
            .map(PathBuf::from)
            .map(|root| scan_local_model_files_from_root(&root))
            .transpose()
            .map_err(ApiError::bad_request)?
            .unwrap_or_default();
        let from_service = list_openai_compatible_models(&endpoint, None)
            .await
            .unwrap_or_default();
        return Ok(Json(merge_model_candidates(from_folder, from_service)));
    }
    let models = fetch_provider_models(
        provider,
        &endpoint,
        api_key.as_deref(),
        models_root.as_deref(),
    )
    .await
    .map_err(|err| ApiError::internal(format!("{}: {}", err.code, err.message)))?;
    Ok(Json(models))
}

pub(crate) async fn probe_model_provider_handler(
    State(state): State<ServerState>,
    Json(payload): Json<ProbeProviderRequest>,
) -> Result<Json<ModelAvailabilityDto>, ApiError> {
    let settings = load_app_settings().map_err(ApiError::internal)?;
    let policy = resolve_enterprise_policy(&settings);
    let provider = ModelProvider::from_value(&payload.provider);
    let endpoint = normalize_endpoint(provider, &payload.endpoint);
    let api_key = normalize_optional_text(payload.api_key);
    let models_root = normalize_optional_text(payload.models_root);
    if let Err(violation) =
        validate_provider_request(&to_model_policy(&policy), provider, &endpoint, &[])
    {
        append_policy_violation_audit(
            &state,
            "anonymous".to_string(),
            "probe_model_provider",
            Some(provider),
            Some(&endpoint),
            &[],
            &violation.message,
        )
        .await;
        return Err(ApiError::forbidden(violation.message));
    }
    let result = fetch_provider_models(
        provider,
        &endpoint,
        api_key.as_deref(),
        models_root.as_deref(),
    )
    .await;
    match result {
        Ok(models) => Ok(Json(ModelAvailabilityDto {
            reachable: true,
            models: models.merged,
            missing_roles: Vec::new(),
            errors: Vec::new(),
            checked_provider: Some(provider_to_string(provider)),
        })),
        Err(err) => Ok(Json(ModelAvailabilityDto {
            reachable: false,
            models: Vec::new(),
            missing_roles: Vec::new(),
            errors: vec![ModelErrorItem {
                code: err.code,
                message: err.message,
            }],
            checked_provider: Some(provider_to_string(provider)),
        })),
    }
}

pub(crate) async fn pull_model_handler(
    State(state): State<ServerState>,
    Json(payload): Json<PullModelRequest>,
) -> Result<Json<ModelAvailabilityDto>, ApiError> {
    let model = payload.model.trim().to_string();
    if model.is_empty() {
        return Err(ApiError::bad_request("model name cannot be empty"));
    }
    let provider = ModelProvider::from_value(&payload.provider);
    let endpoint = normalize_endpoint(provider, &payload.endpoint);
    let policy = resolve_enterprise_policy(&load_app_settings().map_err(ApiError::internal)?);
    if let Err(violation) =
        validate_provider_request(&to_model_policy(&policy), provider, &endpoint, &[])
    {
        append_policy_violation_audit(
            &state,
            "anonymous".to_string(),
            "pull_model",
            Some(provider),
            Some(&endpoint),
            &[],
            &violation.message,
        )
        .await;
        return Err(ApiError::forbidden(violation.message));
    }
    let api_key = normalize_optional_text(payload.api_key);
    pull_llama_cpp_model(&endpoint, &model, api_key.as_deref())
        .await
        .map_err(ApiError::bad_request)?;
    validate_model_setup_handler(State(state)).await
}

pub(crate) async fn set_local_models_root_handler(
    Json(payload): Json<SetLocalModelsRootRequest>,
) -> Result<Json<ModelSettingsDto>, ApiError> {
    let mut settings = load_app_settings().map_err(ApiError::internal)?;
    let path = normalize_optional_text(Some(payload.path));
    if let Some(root_path) = path.as_deref() {
        let root = PathBuf::from(root_path);
        if !root.exists() {
            return Err(ApiError::bad_request(format!(
                "models root does not exist: {}",
                root.display()
            )));
        }
        if !root.is_dir() {
            return Err(ApiError::bad_request(format!(
                "path is not a directory: {}",
                root.display()
            )));
        }
        settings.local_models_root = Some(
            root.canonicalize()
                .unwrap_or(root)
                .to_string_lossy()
                .to_string(),
        );
    } else {
        settings.local_models_root = None;
    }
    save_app_settings(&settings).map_err(ApiError::internal)?;
    Ok(Json(resolve_model_settings(&settings)))
}

pub(crate) async fn scan_local_model_files_handler(
    Json(payload): Json<ScanLocalModelFilesRequest>,
) -> Result<Json<Vec<String>>, ApiError> {
    let root = normalize_optional_text(payload.root);
    if let Some(root) = root {
        let models = scan_local_model_files_from_root(&PathBuf::from(root))
            .map_err(ApiError::bad_request)?;
        return Ok(Json(models));
    }
    let settings = load_app_settings().map_err(ApiError::internal)?;
    let model_settings = resolve_model_settings(&settings);
    if let Some(root) = model_settings.local_profile.models_root {
        let models = scan_local_model_files_from_root(&PathBuf::from(root))
            .map_err(ApiError::bad_request)?;
        return Ok(Json(models));
    }
    Ok(Json(Vec::new()))
}
