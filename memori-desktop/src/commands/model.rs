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
    let mut settings = load_app_settings()?;
    let normalized = normalize_model_settings_payload(payload)?;
    let policy = resolve_enterprise_policy(&settings);
    validate_runtime_model_settings(
        &to_model_policy(&policy),
        &to_runtime_model_config(&resolve_active_runtime_settings(&normalized)),
    )
    .map_err(|violation| violation.message)?;
    settings.active_provider = Some(normalized.active_provider.clone());
    settings.local_endpoint = Some(normalized.local_profile.endpoint.clone());
    settings.local_models_root = normalized.local_profile.models_root.clone();
    settings.local_chat_model = Some(normalized.local_profile.chat_model.clone());
    settings.local_graph_model = Some(normalized.local_profile.graph_model.clone());
    settings.local_embed_model = Some(normalized.local_profile.embed_model.clone());
    settings.remote_endpoint = Some(normalized.remote_profile.endpoint.clone());
    settings.remote_api_key = normalized.remote_profile.api_key.clone();
    settings.remote_chat_model = Some(normalized.remote_profile.chat_model.clone());
    settings.remote_graph_model = Some(normalized.remote_profile.graph_model.clone());
    settings.remote_embed_model = Some(normalized.remote_profile.embed_model.clone());
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
pub(crate) async fn list_provider_models(
    provider: String,
    endpoint: String,
    api_key: Option<String>,
    models_root: Option<String>,
) -> Result<ProviderModelsDto, String> {
    let settings = load_app_settings()?;
    let policy = resolve_enterprise_policy(&settings);
    let provider = ModelProvider::from_value(&provider);
    let endpoint = normalize_endpoint(provider, &endpoint);
    let api_key = normalize_optional_text(api_key);
    let models_root = normalize_optional_text(models_root);
    validate_provider_request(&to_model_policy(&policy), provider, &endpoint, &[])
        .map_err(|violation| violation.message)?;
    if provider == ModelProvider::OllamaLocal {
        let from_folder = models_root
            .as_deref()
            .map(PathBuf::from)
            .map(|root| scan_local_model_files_from_root(&root))
            .transpose()
            .map_err(|err| format!("models_root_invalid: {err}"))?
            .unwrap_or_default();
        let from_service = list_ollama_models(&endpoint).await.unwrap_or_default();
        return Ok(merge_model_candidates(from_folder, from_service));
    }
    fetch_provider_models(
        provider,
        &endpoint,
        api_key.as_deref(),
        models_root.as_deref(),
    )
    .await
    .map_err(|err| format!("{}: {}", err.code, err.message))
}

#[tauri::command]
pub(crate) async fn probe_model_provider(
    provider: String,
    endpoint: String,
    api_key: Option<String>,
    models_root: Option<String>,
) -> Result<ModelAvailabilityDto, String> {
    let settings = load_app_settings()?;
    let policy = resolve_enterprise_policy(&settings);
    let provider = ModelProvider::from_value(&provider);
    let endpoint = normalize_endpoint(provider, &endpoint);
    let api_key = normalize_optional_text(api_key);
    let models_root = normalize_optional_text(models_root);
    validate_provider_request(&to_model_policy(&policy), provider, &endpoint, &[])
        .map_err(|violation| violation.message)?;
    let result = fetch_provider_models(
        provider,
        &endpoint,
        api_key.as_deref(),
        models_root.as_deref(),
    )
    .await;
    match result {
        Ok(models) => Ok(ModelAvailabilityDto {
            configured: true,
            reachable: true,
            models: models.merged,
            missing_roles: Vec::new(),
            errors: Vec::new(),
            checked_provider: Some(provider_to_string(provider)),
            status_code: None,
            status_message: None,
        }),
        Err(err) => Ok(ModelAvailabilityDto {
            configured: true,
            reachable: false,
            models: Vec::new(),
            missing_roles: Vec::new(),
            errors: vec![ModelErrorItem {
                code: err.code,
                message: err.message,
            }],
            checked_provider: Some(provider_to_string(provider)),
            status_code: None,
            status_message: None,
        }),
    }
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
    let models = fetch_provider_models(
        provider,
        &active.endpoint,
        active.api_key.as_deref(),
        active.models_root.as_deref(),
    )
    .await;

    match models {
        Ok(models) => {
            let merged = models.merged;
            let mut missing_roles = Vec::new();
            let mut errors = Vec::new();
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
                    &active.endpoint,
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
        Err(err) => Ok(ModelAvailabilityDto {
            configured: true,
            reachable: false,
            models: Vec::new(),
            missing_roles: vec!["chat".to_string(), "graph".to_string(), "embed".to_string()],
            errors: vec![ModelErrorItem {
                code: err.code,
                message: err.message,
            }],
            checked_provider: Some(provider_to_string(provider)),
            status_code: None,
            status_message: None,
        }),
    }
}

#[tauri::command]
pub(crate) async fn pull_model(
    model: String,
    provider: String,
    endpoint: String,
    api_key: Option<String>,
) -> Result<ModelAvailabilityDto, String> {
    let model = model.trim().to_string();
    if model.is_empty() {
        return Err("模型名不能为空".to_string());
    }
    let provider = ModelProvider::from_value(&provider);
    if provider != ModelProvider::OllamaLocal {
        return Err("仅本地 Ollama 模式支持拉取模型".to_string());
    }
    let endpoint = normalize_endpoint(provider, &endpoint);
    let settings = load_app_settings()?;
    let policy = resolve_enterprise_policy(&settings);
    validate_provider_request(&to_model_policy(&policy), provider, &endpoint, &[])
        .map_err(|violation| violation.message)?;
    let api_key = normalize_optional_text(api_key);
    pull_ollama_model(&endpoint, &model, api_key.as_deref()).await?;
    validate_model_setup().await
}

#[tauri::command]
pub(crate) async fn set_local_models_root(path: String) -> Result<ModelSettingsDto, String> {
    let mut settings = load_app_settings()?;
    let root = normalize_optional_text(Some(path));
    if let Some(root_path) = root.as_deref() {
        let path = PathBuf::from(root_path);
        if !path.exists() {
            return Err(format!("模型目录不存在: {}", path.display()));
        }
        if !path.is_dir() {
            return Err(format!("路径不是目录: {}", path.display()));
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
