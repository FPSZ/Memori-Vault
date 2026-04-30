use crate::*;

pub(crate) const DEFAULT_LOCAL_CHAT_CONTEXT_LENGTH: u32 = 16_384;
pub(crate) const DEFAULT_LOCAL_GRAPH_CONTEXT_LENGTH: u32 = 4_096;
pub(crate) const DEFAULT_LOCAL_EMBED_CONTEXT_LENGTH: u32 = 8_192;

pub(crate) async fn replace_engine(
    engine_slot: &Arc<Mutex<Option<MemoriEngine>>>,
    init_error: &Arc<Mutex<Option<String>>>,
    watch_root: PathBuf,
    reason: &str,
) -> Result<(), String> {
    let previous_engine = {
        let mut guard = engine_slot.lock().await;
        guard.take()
    };

    if let Some(engine) = previous_engine {
        match timeout(
            Duration::from_secs(ENGINE_SHUTDOWN_TIMEOUT_SECS),
            engine.shutdown(),
        )
        .await
        {
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

    let result: Result<(), String> = async {
        let settings = load_app_settings()?;
        let Some(active_runtime) = resolve_configured_active_runtime_settings(&settings) else {
            {
                let mut init_guard = init_error.lock().await;
                *init_guard = Some(MODEL_NOT_CONFIGURED_MESSAGE.to_string());
            }
            info!(
                reason = reason,
                watch_root = %watch_root.display(),
                "memori-desktop daemon skipped because model runtime is not configured"
            );
            return Ok(());
        };
        let policy = resolve_enterprise_policy(&settings);
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
        // 加载索引筛选配置
        if let Some(ref filter) = settings.index_filter {
            let core_filter = if filter.enabled {
                Some(memori_core::IndexFilterConfig {
                    enabled: filter.enabled,
                    include_extensions: filter.include_extensions.clone(),
                    exclude_extensions: filter.exclude_extensions.clone(),
                    exclude_paths: filter.exclude_paths.clone(),
                    include_paths: filter.include_paths.clone(),
                    min_mtime: filter.min_mtime.clone(),
                    max_mtime: filter.max_mtime.clone(),
                    min_size: filter.min_size,
                    max_size: filter.max_size,
                })
            } else {
                None
            };
            new_engine.set_index_filter_config(core_filter).await;
        }
        new_engine.start_daemon().map_err(|err| err.to_string())?;

        {
            let mut guard = engine_slot.lock().await;
            *guard = Some(new_engine);
        }
        {
            let mut init_guard = init_error.lock().await;
            *init_guard = None;
        }

        info!(
            reason = reason,
            watch_root = %watch_root.display(),
            "memori-desktop daemon started"
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

pub(crate) fn describe_engine_error(err: EngineError) -> String {
    match err {
        EngineError::IndexUnavailable { .. } => {
            "Index upgrade required. Search is temporarily unavailable until a full reindex completes.（索引需要升级，完成全量重建前暂不可检索）".to_string()
        }
        EngineError::IndexRebuildInProgress { .. } => {
            "Index upgrade in progress. Search is temporarily unavailable until reindex completes.（索引升级中，重建完成前暂不可检索）".to_string()
        }
        other => other.to_string(),
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ActiveRuntimeModelSettings {
    pub(crate) provider: ModelProvider,
    pub(crate) chat_endpoint: String,
    pub(crate) graph_endpoint: String,
    pub(crate) embed_endpoint: String,
    pub(crate) api_key: Option<String>,
    pub(crate) models_root: Option<String>,
    pub(crate) chat_model: String,
    pub(crate) graph_model: String,
    pub(crate) embed_model: String,
    pub(crate) chat_context_length: Option<u32>,
    pub(crate) graph_context_length: Option<u32>,
    pub(crate) embed_context_length: Option<u32>,
    pub(crate) chat_concurrency: Option<u32>,
    pub(crate) graph_concurrency: Option<u32>,
    pub(crate) embed_concurrency: Option<u32>,
}

pub(crate) fn to_runtime_model_config(settings: &ActiveRuntimeModelSettings) -> RuntimeModelConfig {
    RuntimeModelConfig {
        provider: settings.provider,
        chat_endpoint: settings.chat_endpoint.clone(),
        chat_model: settings.chat_model.clone(),
        graph_endpoint: settings.graph_endpoint.clone(),
        graph_model: settings.graph_model.clone(),
        embed_endpoint: settings.embed_endpoint.clone(),
        embed_model: settings.embed_model.clone(),
        api_key: settings.api_key.clone(),
        chat_context_length: settings.chat_context_length,
        graph_context_length: settings.graph_context_length,
        embed_context_length: settings.embed_context_length,
        chat_concurrency: settings.chat_concurrency,
        graph_concurrency: settings.graph_concurrency,
        embed_concurrency: settings.embed_concurrency,
    }
}

pub(crate) fn resolve_enterprise_policy(settings: &AppSettings) -> EnterprisePolicyDto {
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
        "llama_cpp_local".to_string()
    }
}

pub(crate) fn resolve_endpoint(
    explicit: Option<String>,
    single: Option<String>,
    legacy: Option<String>,
    specific_env: &str,
    generic_env: &str,
    default: &str,
) -> String {
    explicit
        .or(single)
        .or(legacy)
        .or_else(|| std::env::var(specific_env).ok())
        .or_else(|| std::env::var(generic_env).ok())
        .unwrap_or_else(|| default.to_string())
}

pub(crate) fn normalize_performance_preset(value: Option<String>) -> Option<String> {
    match normalize_optional_text(value)
        .unwrap_or_else(|| "compat".to_string())
        .to_ascii_lowercase()
        .as_str()
    {
        "gpu" => Some("gpu".to_string()),
        "low_vram" | "low-vram" => Some("low_vram".to_string()),
        "throughput" => Some("throughput".to_string()),
        _ => Some("compat".to_string()),
    }
}
