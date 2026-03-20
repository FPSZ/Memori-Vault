use crate::*;

pub(crate) async fn replace_engine(
    engine_slot: &Arc<RwLock<Option<MemoriEngine>>>,
    init_error: &Arc<Mutex<Option<String>>>,
    watch_root: PathBuf,
    reason: &str,
) -> Result<(), String> {
    {
        let mut engine_guard = engine_slot.write().await;
        if let Some(ref engine) = *engine_guard {
            if let Err(err) = timeout(
                Duration::from_secs(ENGINE_SHUTDOWN_TIMEOUT_SECS),
                engine.shutdown(),
            )
            .await
            {
                match err {
                    Ok(Ok(())) => {}
                    Ok(Err(err)) => {
                        warn!(error = %err, "关闭旧引擎失败，继续尝试重建");
                    }
                    Err(_) => {
                        warn!(
                            timeout_secs = ENGINE_SHUTDOWN_TIMEOUT_SECS,
                            "关闭旧引擎超时，继续尝试重建"
                        );
                    }
                }
            }
            *engine_guard = None;
        }
    }

    let result: Result<(), String> = async {
        let settings = load_app_settings()?;
        let policy = resolve_enterprise_policy(&settings);
        let model_settings = resolve_model_settings(&settings);
        let active_runtime = resolve_active_runtime_settings(&model_settings);
        validate_runtime_model_settings(
            &to_model_policy(&policy),
            &to_runtime_model_config(&active_runtime),
        )
        .map_err(|violation| violation.message)?;
        apply_model_settings_to_env(active_runtime);

        let mut new_engine =
            MemoriEngine::bootstrap(watch_root.clone()).map_err(|err| err.to_string())?;
        new_engine
            .set_indexing_config(resolve_indexing_config(&settings))
            .await;
        new_engine.start_daemon().map_err(|err| err.to_string())?;

        {
            let mut guard = engine_slot.write().await;
            *guard = Some(new_engine);
        }
        {
            let mut init_guard = init_error.lock().await;
            *init_guard = None;
        }

        info!(
            reason = reason,
            watch_root = %watch_root.display(),
            "memori-server daemon started"
        );

        Ok(())
    }
    .await;

    if let Err(err) = &result {
        let mut init_guard = init_error.lock().await;
        *init_guard = Some(err.clone());
    }

    result
}

pub(crate) struct ActiveRuntimeModelSettings {
    pub(crate) provider: ModelProvider,
    pub(crate) endpoint: String,
    pub(crate) api_key: Option<String>,
    pub(crate) models_root: Option<String>,
    pub(crate) chat_model: String,
    pub(crate) graph_model: String,
    pub(crate) embed_model: String,
}

pub(crate) fn to_runtime_model_config(settings: &ActiveRuntimeModelSettings) -> RuntimeModelConfig {
    RuntimeModelConfig {
        provider: settings.provider,
        endpoint: settings.endpoint.clone(),
        api_key: settings.api_key.clone(),
        chat_model: settings.chat_model.clone(),
        graph_model: settings.graph_model.clone(),
        embed_model: settings.embed_model.clone(),
    }
}

pub(crate) fn to_model_policy(policy: &EnterprisePolicyDto) -> EnterpriseModelPolicy {
    EnterpriseModelPolicy {
        egress_mode: policy.egress_mode,
        allowed_model_endpoints: policy.allowed_model_endpoints.clone(),
        allowed_models: policy.allowed_models.clone(),
    }
}

pub(crate) fn provider_to_string(provider: ModelProvider) -> String {
    if provider == ModelProvider::OpenAiCompatible {
        "openai_compatible".to_string()
    } else {
        "ollama_local".to_string()
    }
}

pub(crate) fn resolve_model_settings(settings: &AppSettings) -> ModelSettingsDto {
    let fallback_provider = settings.active_provider.clone().unwrap_or_else(|| {
        settings.provider.clone().unwrap_or_else(|| {
            std::env::var(MEMORI_MODEL_PROVIDER_ENV)
                .unwrap_or_else(|_| DEFAULT_MODEL_PROVIDER.to_string())
        })
    });
    let active_provider = ModelProvider::from_value(&fallback_provider);
    let env_provider = std::env::var(MEMORI_MODEL_PROVIDER_ENV)
        .ok()
        .map(|value| ModelProvider::from_value(&value))
        .unwrap_or(active_provider);

    let local_endpoint = settings
        .local_endpoint
        .clone()
        .or_else(|| {
            if ModelProvider::from_value(
                settings
                    .provider
                    .as_deref()
                    .unwrap_or(fallback_provider.as_str()),
            ) == ModelProvider::OllamaLocal
            {
                settings.endpoint.clone()
            } else {
                None
            }
        })
        .or_else(|| {
            if env_provider == ModelProvider::OllamaLocal {
                std::env::var(MEMORI_MODEL_ENDPOINT_ENV).ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| DEFAULT_MODEL_ENDPOINT_OLLAMA.to_string());

    let remote_endpoint = settings
        .remote_endpoint
        .clone()
        .or_else(|| {
            if ModelProvider::from_value(
                settings
                    .provider
                    .as_deref()
                    .unwrap_or(fallback_provider.as_str()),
            ) == ModelProvider::OpenAiCompatible
            {
                settings.endpoint.clone()
            } else {
                None
            }
        })
        .or_else(|| {
            if env_provider == ModelProvider::OpenAiCompatible {
                std::env::var(MEMORI_MODEL_ENDPOINT_ENV).ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| memori_core::DEFAULT_MODEL_ENDPOINT_OPENAI.to_string());

    let local_chat_model = settings
        .local_chat_model
        .clone()
        .or_else(|| {
            if ModelProvider::from_value(
                settings
                    .provider
                    .as_deref()
                    .unwrap_or(fallback_provider.as_str()),
            ) == ModelProvider::OllamaLocal
            {
                settings.chat_model.clone()
            } else {
                None
            }
        })
        .or_else(|| {
            if env_provider == ModelProvider::OllamaLocal {
                std::env::var(MEMORI_CHAT_MODEL_ENV).ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| DEFAULT_CHAT_MODEL.to_string());

    let local_graph_model = settings
        .local_graph_model
        .clone()
        .or_else(|| {
            if ModelProvider::from_value(
                settings
                    .provider
                    .as_deref()
                    .unwrap_or(fallback_provider.as_str()),
            ) == ModelProvider::OllamaLocal
            {
                settings.graph_model.clone()
            } else {
                None
            }
        })
        .or_else(|| {
            if env_provider == ModelProvider::OllamaLocal {
                std::env::var(MEMORI_GRAPH_MODEL_ENV).ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| DEFAULT_GRAPH_MODEL.to_string());

    let local_embed_model = settings
        .local_embed_model
        .clone()
        .or_else(|| {
            if ModelProvider::from_value(
                settings
                    .provider
                    .as_deref()
                    .unwrap_or(fallback_provider.as_str()),
            ) == ModelProvider::OllamaLocal
            {
                settings.embed_model.clone()
            } else {
                None
            }
        })
        .or_else(|| {
            if env_provider == ModelProvider::OllamaLocal {
                std::env::var(MEMORI_EMBED_MODEL_ENV).ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| DEFAULT_OLLAMA_EMBED_MODEL.to_string());

    let remote_chat_model = settings
        .remote_chat_model
        .clone()
        .or_else(|| {
            if ModelProvider::from_value(
                settings
                    .provider
                    .as_deref()
                    .unwrap_or(fallback_provider.as_str()),
            ) == ModelProvider::OpenAiCompatible
            {
                settings.chat_model.clone()
            } else {
                None
            }
        })
        .or_else(|| {
            if env_provider == ModelProvider::OpenAiCompatible {
                std::env::var(MEMORI_CHAT_MODEL_ENV).ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| DEFAULT_CHAT_MODEL.to_string());

    let remote_graph_model = settings
        .remote_graph_model
        .clone()
        .or_else(|| {
            if ModelProvider::from_value(
                settings
                    .provider
                    .as_deref()
                    .unwrap_or(fallback_provider.as_str()),
            ) == ModelProvider::OpenAiCompatible
            {
                settings.graph_model.clone()
            } else {
                None
            }
        })
        .or_else(|| {
            if env_provider == ModelProvider::OpenAiCompatible {
                std::env::var(MEMORI_GRAPH_MODEL_ENV).ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| DEFAULT_GRAPH_MODEL.to_string());

    let remote_embed_model = settings
        .remote_embed_model
        .clone()
        .or_else(|| {
            if ModelProvider::from_value(
                settings
                    .provider
                    .as_deref()
                    .unwrap_or(fallback_provider.as_str()),
            ) == ModelProvider::OpenAiCompatible
            {
                settings.embed_model.clone()
            } else {
                None
            }
        })
        .or_else(|| {
            if env_provider == ModelProvider::OpenAiCompatible {
                std::env::var(MEMORI_EMBED_MODEL_ENV).ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| DEFAULT_OLLAMA_EMBED_MODEL.to_string());

    let remote_api_key = settings
        .remote_api_key
        .clone()
        .or_else(|| {
            if ModelProvider::from_value(
                settings
                    .provider
                    .as_deref()
                    .unwrap_or(fallback_provider.as_str()),
            ) == ModelProvider::OpenAiCompatible
            {
                settings.api_key.clone()
            } else {
                None
            }
        })
        .or_else(|| {
            if env_provider == ModelProvider::OpenAiCompatible {
                std::env::var(MEMORI_MODEL_API_KEY_ENV).ok()
            } else {
                None
            }
        })
        .and_then(|value| normalize_optional_text(Some(value)));

    ModelSettingsDto {
        active_provider: provider_to_string(active_provider),
        local_profile: LocalModelProfileDto {
            endpoint: normalize_endpoint(ModelProvider::OllamaLocal, &local_endpoint),
            models_root: normalize_optional_text(settings.local_models_root.clone()),
            chat_model: local_chat_model,
            graph_model: local_graph_model,
            embed_model: local_embed_model,
        },
        remote_profile: RemoteModelProfileDto {
            endpoint: normalize_endpoint(ModelProvider::OpenAiCompatible, &remote_endpoint),
            api_key: remote_api_key,
            chat_model: remote_chat_model,
            graph_model: remote_graph_model,
            embed_model: remote_embed_model,
        },
    }
}

pub(crate) fn normalize_model_settings_payload(
    payload: ModelSettingsDto,
) -> Result<ModelSettingsDto, String> {
    let active_provider = ModelProvider::from_value(&payload.active_provider);
    let local_endpoint =
        normalize_endpoint(ModelProvider::OllamaLocal, &payload.local_profile.endpoint);
    let remote_endpoint = normalize_endpoint(
        ModelProvider::OpenAiCompatible,
        &payload.remote_profile.endpoint,
    );

    let local_chat_model = payload.local_profile.chat_model.trim().to_string();
    let local_graph_model = payload.local_profile.graph_model.trim().to_string();
    let local_embed_model = payload.local_profile.embed_model.trim().to_string();
    let remote_chat_model = payload.remote_profile.chat_model.trim().to_string();
    let remote_graph_model = payload.remote_profile.graph_model.trim().to_string();
    let remote_embed_model = payload.remote_profile.embed_model.trim().to_string();

    if local_chat_model.is_empty()
        || local_graph_model.is_empty()
        || local_embed_model.is_empty()
        || remote_chat_model.is_empty()
        || remote_graph_model.is_empty()
        || remote_embed_model.is_empty()
    {
        return Err("chat/graph/embed 模型名均不能为空".to_string());
    }

    let local_models_root =
        normalize_optional_text(payload.local_profile.models_root).map(|path| {
            let p = PathBuf::from(&path);
            p.canonicalize().unwrap_or(p).to_string_lossy().to_string()
        });

    Ok(ModelSettingsDto {
        active_provider: provider_to_string(active_provider),
        local_profile: LocalModelProfileDto {
            endpoint: local_endpoint,
            models_root: local_models_root,
            chat_model: local_chat_model,
            graph_model: local_graph_model,
            embed_model: local_embed_model,
        },
        remote_profile: RemoteModelProfileDto {
            endpoint: remote_endpoint,
            api_key: normalize_optional_text(payload.remote_profile.api_key),
            chat_model: remote_chat_model,
            graph_model: remote_graph_model,
            embed_model: remote_embed_model,
        },
    })
}

pub(crate) fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|v| {
        let trimmed = v.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

pub(crate) fn normalize_endpoint(provider: ModelProvider, endpoint: &str) -> String {
    let trimmed = endpoint.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }
    if provider == ModelProvider::OpenAiCompatible {
        memori_core::DEFAULT_MODEL_ENDPOINT_OPENAI.to_string()
    } else {
        DEFAULT_MODEL_ENDPOINT_OLLAMA.to_string()
    }
}

pub(crate) fn resolve_active_runtime_settings(
    settings: &ModelSettingsDto,
) -> ActiveRuntimeModelSettings {
    let active_provider = ModelProvider::from_value(&settings.active_provider);
    if active_provider == ModelProvider::OpenAiCompatible {
        return ActiveRuntimeModelSettings {
            provider: ModelProvider::OpenAiCompatible,
            endpoint: normalize_endpoint(
                ModelProvider::OpenAiCompatible,
                &settings.remote_profile.endpoint,
            ),
            api_key: normalize_optional_text(settings.remote_profile.api_key.clone()),
            models_root: None,
            chat_model: settings.remote_profile.chat_model.trim().to_string(),
            graph_model: settings.remote_profile.graph_model.trim().to_string(),
            embed_model: settings.remote_profile.embed_model.trim().to_string(),
        };
    }

    ActiveRuntimeModelSettings {
        provider: ModelProvider::OllamaLocal,
        endpoint: normalize_endpoint(ModelProvider::OllamaLocal, &settings.local_profile.endpoint),
        api_key: None,
        models_root: normalize_optional_text(settings.local_profile.models_root.clone()),
        chat_model: settings.local_profile.chat_model.trim().to_string(),
        graph_model: settings.local_profile.graph_model.trim().to_string(),
        embed_model: settings.local_profile.embed_model.trim().to_string(),
    }
}

pub(crate) fn apply_model_settings_to_env(settings: ActiveRuntimeModelSettings) {
    // SAFETY: process-global config source for memori-core runtime.
    unsafe {
        std::env::set_var(
            MEMORI_MODEL_PROVIDER_ENV,
            provider_to_string(settings.provider),
        );
        std::env::set_var(MEMORI_MODEL_ENDPOINT_ENV, &settings.endpoint);
        std::env::set_var(MEMORI_CHAT_MODEL_ENV, &settings.chat_model);
        std::env::set_var(MEMORI_GRAPH_MODEL_ENV, &settings.graph_model);
        std::env::set_var(MEMORI_EMBED_MODEL_ENV, &settings.embed_model);
        if let Some(key) = settings.api_key.as_ref() {
            std::env::set_var(MEMORI_MODEL_API_KEY_ENV, key);
        } else {
            std::env::remove_var(MEMORI_MODEL_API_KEY_ENV);
        }
    }
}

pub(crate) async fn fetch_provider_models(
    provider: ModelProvider,
    endpoint: &str,
    api_key: Option<&str>,
    models_root: Option<&str>,
) -> Result<ProviderModelsDto, ProviderModelFetchError> {
    match provider {
        ModelProvider::OllamaLocal => {
            let from_folder = models_root
                .map(PathBuf::from)
                .map(|root| scan_local_model_files_from_root(&root))
                .transpose()
                .map_err(|err| ProviderModelFetchError {
                    code: "models_root_invalid".to_string(),
                    message: err,
                })?
                .unwrap_or_default();
            let from_service = list_ollama_models(endpoint).await?;
            Ok(merge_model_candidates(from_folder, from_service))
        }
        ModelProvider::OpenAiCompatible => {
            let from_service = list_openai_compatible_models(endpoint, api_key).await?;
            Ok(merge_model_candidates(Vec::new(), from_service))
        }
    }
}

pub(crate) fn merge_model_candidates(
    from_folder: Vec<String>,
    from_service: Vec<String>,
) -> ProviderModelsDto {
    let mut merged_set = BTreeSet::new();
    for model in &from_folder {
        merged_set.insert(model.clone());
    }
    for model in &from_service {
        merged_set.insert(model.clone());
    }
    ProviderModelsDto {
        from_folder,
        from_service,
        merged: merged_set.into_iter().collect(),
    }
}

pub(crate) async fn list_ollama_models(
    endpoint: &str,
) -> Result<Vec<String>, ProviderModelFetchError> {
    #[derive(Debug, Deserialize)]
    struct OllamaTagResp {
        models: Vec<OllamaTagItem>,
    }
    #[derive(Debug, Deserialize)]
    struct OllamaTagItem {
        name: String,
    }
    let url = format!("{}/api/tags", endpoint.trim_end_matches('/'));
    let response = timeout(
        Duration::from_secs(PROVIDER_HTTP_TIMEOUT_SECS),
        reqwest::Client::new().get(url).send(),
    )
    .await
    .map_err(|_| ProviderModelFetchError {
        code: "request_timeout".to_string(),
        message: format!("连接 Ollama 超时({}s)", PROVIDER_HTTP_TIMEOUT_SECS),
    })?
    .map_err(|err| ProviderModelFetchError {
        code: "endpoint_unreachable".to_string(),
        message: format!("连接 Ollama 失败: {err}"),
    })?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(ProviderModelFetchError {
            code: "endpoint_unreachable".to_string(),
            message: format!("Ollama 模型列表请求失败: status={}, body={body}", status),
        });
    }
    let parsed: OllamaTagResp = response
        .json()
        .await
        .map_err(|err| ProviderModelFetchError {
            code: "endpoint_unreachable".to_string(),
            message: format!("解析 Ollama 模型列表失败: {err}"),
        })?;
    Ok(parsed.models.into_iter().map(|m| m.name).collect())
}

pub(crate) async fn list_openai_compatible_models(
    endpoint: &str,
    api_key: Option<&str>,
) -> Result<Vec<String>, ProviderModelFetchError> {
    #[derive(Debug, Deserialize)]
    struct OpenAiModelsResp {
        data: Vec<OpenAiModelItem>,
    }
    #[derive(Debug, Deserialize)]
    struct OpenAiModelItem {
        id: String,
    }
    let url = format!("{}/v1/models", endpoint.trim_end_matches('/'));
    let mut request = reqwest::Client::new().get(url);
    if let Some(key) = api_key {
        request = request.bearer_auth(key);
    }
    let response = timeout(
        Duration::from_secs(PROVIDER_HTTP_TIMEOUT_SECS),
        request.send(),
    )
    .await
    .map_err(|_| ProviderModelFetchError {
        code: "request_timeout".to_string(),
        message: format!("连接远程模型服务超时({}s)", PROVIDER_HTTP_TIMEOUT_SECS),
    })?
    .map_err(|err| ProviderModelFetchError {
        code: "endpoint_unreachable".to_string(),
        message: format!("连接远程模型服务失败: {err}"),
    })?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let code = if status.as_u16() == 401 || status.as_u16() == 403 {
            "auth_failed"
        } else {
            "endpoint_unreachable"
        };
        return Err(ProviderModelFetchError {
            code: code.to_string(),
            message: format!("status={}, body={body}", status),
        });
    }
    let parsed: OpenAiModelsResp =
        response
            .json()
            .await
            .map_err(|err| ProviderModelFetchError {
                code: "endpoint_unreachable".to_string(),
                message: format!("解析远程模型列表失败: {err}"),
            })?;
    Ok(parsed.data.into_iter().map(|m| m.id).collect())
}

pub(crate) fn scan_local_model_files_from_root(root: &Path) -> Result<Vec<String>, String> {
    if !root.exists() {
        return Err(format!("模型目录不存在: {}", root.display()));
    }
    if !root.is_dir() {
        return Err(format!("路径不是目录: {}", root.display()));
    }
    let mut set = BTreeSet::new();
    collect_local_model_files_recursive(root, &mut set, 0, 8)?;
    Ok(set.into_iter().collect())
}

pub(crate) fn collect_local_model_files_recursive(
    dir: &Path,
    set: &mut BTreeSet<String>,
    depth: usize,
    max_depth: usize,
) -> Result<(), String> {
    if depth > max_depth {
        return Ok(());
    }
    let entries =
        fs::read_dir(dir).map_err(|err| format!("读取模型目录失败({}): {err}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("读取模型目录项失败({}): {err}", dir.display()))?;
        let path = entry.path();
        let metadata = entry
            .metadata()
            .map_err(|err| format!("读取模型目录元数据失败({}): {err}", path.display()))?;
        if metadata.is_dir() {
            collect_local_model_files_recursive(&path, set, depth + 1, max_depth)?;
            continue;
        }
        if !metadata.is_file() {
            continue;
        }
        let Some(ext) = path.extension().and_then(|v| v.to_str()) else {
            continue;
        };
        if !ext.eq_ignore_ascii_case("gguf") {
            continue;
        }
        if let Some(name) = path.file_stem().and_then(|v| v.to_str()) {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                set.insert(trimmed.to_string());
            }
        }
    }
    Ok(())
}

pub(crate) async fn pull_ollama_model(
    endpoint: &str,
    model: &str,
    _api_key: Option<&str>,
) -> Result<(), String> {
    #[derive(Debug, Serialize)]
    struct PullBody<'a> {
        name: &'a str,
        stream: bool,
    }
    let url = format!("{}/api/pull", endpoint.trim_end_matches('/'));
    let response = timeout(
        Duration::from_secs(PROVIDER_HTTP_TIMEOUT_SECS),
        reqwest::Client::new()
            .post(url)
            .json(&PullBody {
                name: model,
                stream: false,
            })
            .send(),
    )
    .await
    .map_err(|_| format!("拉取模型超时({}s)", PROVIDER_HTTP_TIMEOUT_SECS))?
    .map_err(|err| format!("拉取模型失败: {err}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("拉取模型失败: status={}, body={body}", status));
    }
    Ok(())
}

pub(crate) fn model_exists(models: &[String], expected: &str) -> bool {
    let expected = expected.trim();
    if expected.is_empty() {
        return false;
    }
    models.iter().any(|m| m == expected)
        || (!expected.contains(':') && models.iter().any(|m| m == &format!("{expected}:latest")))
}

pub(crate) fn resolve_enterprise_policy(settings: &AppSettings) -> EnterprisePolicyDto {
    let indexing = resolve_indexing_config(settings);
    let allowed_model_endpoints = settings
        .enterprise_allowed_model_endpoints
        .clone()
        .unwrap_or_default()
        .into_iter()
        .map(|item| normalize_policy_endpoint(&item))
        .filter(|item| !item.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let allowed_models = settings
        .enterprise_allowed_models
        .clone()
        .unwrap_or_default()
        .into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    EnterprisePolicyDto {
        egress_mode: settings
            .enterprise_egress_mode
            .as_deref()
            .map(EgressMode::from_value)
            .unwrap_or_default(),
        allowed_model_endpoints,
        allowed_models,
        indexing_default_mode: indexing.mode.as_str().to_string(),
        resource_budget_default: indexing.resource_budget.as_str().to_string(),
        auth: AuthConfigDto {
            issuer: settings
                .oidc_issuer
                .clone()
                .unwrap_or_else(|| "https://example-idp.local".to_string()),
            client_id: settings
                .oidc_client_id
                .clone()
                .unwrap_or_else(|| "memori-vault-enterprise".to_string()),
            redirect_uri: settings
                .oidc_redirect_uri
                .clone()
                .unwrap_or_else(|| "http://localhost:3757/api/auth/oidc/login".to_string()),
            roles_claim: settings
                .oidc_roles_claim
                .clone()
                .unwrap_or_else(|| "roles".to_string()),
        },
    }
}
