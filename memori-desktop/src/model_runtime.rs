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

fn resolve_endpoint(
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

    let local_chat_endpoint = resolve_endpoint(
        settings.local_chat_endpoint.clone(),
        settings.local_endpoint.clone(),
        settings.endpoint.clone(),
        MEMORI_CHAT_ENDPOINT_ENV,
        MEMORI_MODEL_ENDPOINT_ENV,
        DEFAULT_CHAT_ENDPOINT,
    );
    let local_graph_endpoint = resolve_endpoint(
        settings.local_graph_endpoint.clone(),
        settings.local_endpoint.clone(),
        settings.endpoint.clone(),
        MEMORI_GRAPH_ENDPOINT_ENV,
        MEMORI_MODEL_ENDPOINT_ENV,
        DEFAULT_GRAPH_ENDPOINT,
    );
    let local_embed_endpoint = resolve_endpoint(
        settings.local_embed_endpoint.clone(),
        settings.local_endpoint.clone(),
        settings.endpoint.clone(),
        MEMORI_EMBED_ENDPOINT_ENV,
        MEMORI_MODEL_ENDPOINT_ENV,
        DEFAULT_EMBED_ENDPOINT,
    );

    let remote_chat_endpoint = resolve_endpoint(
        settings.remote_chat_endpoint.clone(),
        settings.remote_endpoint.clone(),
        settings.endpoint.clone(),
        MEMORI_CHAT_ENDPOINT_ENV,
        MEMORI_MODEL_ENDPOINT_ENV,
        DEFAULT_CHAT_ENDPOINT,
    );
    let remote_graph_endpoint = resolve_endpoint(
        settings.remote_graph_endpoint.clone(),
        settings.remote_endpoint.clone(),
        settings.endpoint.clone(),
        MEMORI_GRAPH_ENDPOINT_ENV,
        MEMORI_MODEL_ENDPOINT_ENV,
        DEFAULT_GRAPH_ENDPOINT,
    );
    let remote_embed_endpoint = resolve_endpoint(
        settings.remote_embed_endpoint.clone(),
        settings.remote_endpoint.clone(),
        settings.endpoint.clone(),
        MEMORI_EMBED_ENDPOINT_ENV,
        MEMORI_MODEL_ENDPOINT_ENV,
        DEFAULT_EMBED_ENDPOINT,
    );

    let local_chat_model = settings
        .local_chat_model
        .clone()
        .or_else(|| {
            if ModelProvider::from_value(
                settings
                    .provider
                    .as_deref()
                    .unwrap_or(fallback_provider.as_str()),
            ) == ModelProvider::LlamaCppLocal
            {
                settings.chat_model.clone()
            } else {
                None
            }
        })
        .or_else(|| {
            if env_provider == ModelProvider::LlamaCppLocal {
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
            ) == ModelProvider::LlamaCppLocal
            {
                settings.graph_model.clone()
            } else {
                None
            }
        })
        .or_else(|| {
            if env_provider == ModelProvider::LlamaCppLocal {
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
            ) == ModelProvider::LlamaCppLocal
            {
                settings.embed_model.clone()
            } else {
                None
            }
        })
        .or_else(|| {
            if env_provider == ModelProvider::LlamaCppLocal {
                std::env::var(MEMORI_EMBED_MODEL_ENV).ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| DEFAULT_EMBED_MODEL_QWEN3.to_string());

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
        .unwrap_or_else(|| DEFAULT_EMBED_MODEL_QWEN3.to_string());

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
            chat_endpoint: normalize_endpoint(ModelProvider::LlamaCppLocal, &local_chat_endpoint),
            graph_endpoint: normalize_endpoint(ModelProvider::LlamaCppLocal, &local_graph_endpoint),
            embed_endpoint: normalize_endpoint(ModelProvider::LlamaCppLocal, &local_embed_endpoint),
            models_root: normalize_optional_text(settings.local_models_root.clone()),
            llama_server_path: normalize_optional_text(settings.local_llama_server_path.clone()),
            chat_model: local_chat_model,
            graph_model: local_graph_model,
            embed_model: local_embed_model,
            chat_model_path: normalize_optional_text(settings.local_chat_model_path.clone()),
            graph_model_path: normalize_optional_text(settings.local_graph_model_path.clone()),
            embed_model_path: normalize_optional_text(settings.local_embed_model_path.clone()),
            chat_context_length: Some(
                settings
                    .local_chat_context_length
                    .unwrap_or(DEFAULT_LOCAL_CHAT_CONTEXT_LENGTH),
            ),
            graph_context_length: Some(
                settings
                    .local_graph_context_length
                    .unwrap_or(DEFAULT_LOCAL_GRAPH_CONTEXT_LENGTH),
            ),
            embed_context_length: Some(
                settings
                    .local_embed_context_length
                    .unwrap_or(DEFAULT_LOCAL_EMBED_CONTEXT_LENGTH),
            ),
            chat_concurrency: settings.local_chat_concurrency,
            graph_concurrency: settings.local_graph_concurrency,
            embed_concurrency: settings.local_embed_concurrency,
            performance_preset: normalize_performance_preset(
                settings.local_performance_preset.clone(),
            ),
            n_gpu_layers: settings.local_n_gpu_layers,
            batch_size: settings.local_batch_size,
            ubatch_size: settings.local_ubatch_size,
            threads: settings.local_threads,
            threads_batch: settings.local_threads_batch,
            flash_attn: settings.local_flash_attn,
            cache_type_k: normalize_optional_text(settings.local_cache_type_k.clone()),
            cache_type_v: normalize_optional_text(settings.local_cache_type_v.clone()),
        },
        remote_profile: RemoteModelProfileDto {
            chat_endpoint: normalize_endpoint(
                ModelProvider::OpenAiCompatible,
                &remote_chat_endpoint,
            ),
            graph_endpoint: normalize_endpoint(
                ModelProvider::OpenAiCompatible,
                &remote_graph_endpoint,
            ),
            embed_endpoint: normalize_endpoint(
                ModelProvider::OpenAiCompatible,
                &remote_embed_endpoint,
            ),
            api_key: remote_api_key,
            chat_model: remote_chat_model,
            graph_model: remote_graph_model,
            embed_model: remote_embed_model,
            chat_context_length: settings.remote_chat_context_length,
            graph_context_length: settings.remote_graph_context_length,
            embed_context_length: settings.remote_embed_context_length,
            chat_concurrency: settings.remote_chat_concurrency,
            graph_concurrency: settings.remote_graph_concurrency,
            embed_concurrency: settings.remote_embed_concurrency,
        },
        stop_local_models_on_exit: settings.stop_local_models_on_exit.unwrap_or(true),
    }
}

pub(crate) fn normalize_model_settings_payload(
    payload: ModelSettingsDto,
) -> Result<ModelSettingsDto, String> {
    let active_provider = ModelProvider::from_value(&payload.active_provider);
    let local_chat_endpoint = normalize_endpoint(
        ModelProvider::LlamaCppLocal,
        &payload.local_profile.chat_endpoint,
    );
    let local_graph_endpoint = normalize_endpoint(
        ModelProvider::LlamaCppLocal,
        &payload.local_profile.graph_endpoint,
    );
    let local_embed_endpoint = normalize_endpoint(
        ModelProvider::LlamaCppLocal,
        &payload.local_profile.embed_endpoint,
    );
    let remote_chat_endpoint = normalize_endpoint(
        ModelProvider::OpenAiCompatible,
        &payload.remote_profile.chat_endpoint,
    );
    let remote_graph_endpoint = normalize_endpoint(
        ModelProvider::OpenAiCompatible,
        &payload.remote_profile.graph_endpoint,
    );
    let remote_embed_endpoint = normalize_endpoint(
        ModelProvider::OpenAiCompatible,
        &payload.remote_profile.embed_endpoint,
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
    let local_llama_server_path =
        normalize_optional_existing_file(payload.local_profile.llama_server_path, "llama-server")?;
    let local_chat_model_path =
        normalize_optional_existing_file(payload.local_profile.chat_model_path, "chat model")?;
    let local_graph_model_path =
        normalize_optional_existing_file(payload.local_profile.graph_model_path, "graph model")?;
    let local_embed_model_path =
        normalize_optional_existing_file(payload.local_profile.embed_model_path, "embed model")?;
    let local_performance_preset =
        normalize_performance_preset(payload.local_profile.performance_preset);
    let local_cache_type_k = normalize_optional_text(payload.local_profile.cache_type_k);
    let local_cache_type_v = normalize_optional_text(payload.local_profile.cache_type_v);

    Ok(ModelSettingsDto {
        active_provider: provider_to_string(active_provider),
        local_profile: LocalModelProfileDto {
            chat_endpoint: local_chat_endpoint,
            graph_endpoint: local_graph_endpoint,
            embed_endpoint: local_embed_endpoint,
            models_root: local_models_root,
            llama_server_path: local_llama_server_path,
            chat_model: local_chat_model,
            graph_model: local_graph_model,
            embed_model: local_embed_model,
            chat_model_path: local_chat_model_path,
            graph_model_path: local_graph_model_path,
            embed_model_path: local_embed_model_path,
            chat_context_length: payload.local_profile.chat_context_length,
            graph_context_length: payload.local_profile.graph_context_length,
            embed_context_length: payload.local_profile.embed_context_length,
            chat_concurrency: payload.local_profile.chat_concurrency,
            graph_concurrency: payload.local_profile.graph_concurrency,
            embed_concurrency: payload.local_profile.embed_concurrency,
            performance_preset: local_performance_preset,
            n_gpu_layers: payload.local_profile.n_gpu_layers,
            batch_size: payload.local_profile.batch_size,
            ubatch_size: payload.local_profile.ubatch_size,
            threads: payload.local_profile.threads,
            threads_batch: payload.local_profile.threads_batch,
            flash_attn: payload.local_profile.flash_attn,
            cache_type_k: local_cache_type_k,
            cache_type_v: local_cache_type_v,
        },
        remote_profile: RemoteModelProfileDto {
            chat_endpoint: remote_chat_endpoint,
            graph_endpoint: remote_graph_endpoint,
            embed_endpoint: remote_embed_endpoint,
            api_key: normalize_optional_text(payload.remote_profile.api_key),
            chat_model: remote_chat_model,
            graph_model: remote_graph_model,
            embed_model: remote_embed_model,
            chat_context_length: payload.remote_profile.chat_context_length,
            graph_context_length: payload.remote_profile.graph_context_length,
            embed_context_length: payload.remote_profile.embed_context_length,
            chat_concurrency: payload.remote_profile.chat_concurrency,
            graph_concurrency: payload.remote_profile.graph_concurrency,
            embed_concurrency: payload.remote_profile.embed_concurrency,
        },
        stop_local_models_on_exit: payload.stop_local_models_on_exit,
    })
}

pub(crate) fn resolve_active_runtime_settings(
    settings: &ModelSettingsDto,
) -> ActiveRuntimeModelSettings {
    let active_provider = ModelProvider::from_value(&settings.active_provider);

    ActiveRuntimeModelSettings {
        provider: active_provider,
        chat_endpoint: if active_provider == ModelProvider::OpenAiCompatible {
            normalize_endpoint(
                ModelProvider::OpenAiCompatible,
                &settings.remote_profile.chat_endpoint,
            )
        } else {
            normalize_endpoint(
                ModelProvider::LlamaCppLocal,
                &settings.local_profile.chat_endpoint,
            )
        },
        graph_endpoint: if active_provider == ModelProvider::OpenAiCompatible {
            normalize_endpoint(
                ModelProvider::OpenAiCompatible,
                &settings.remote_profile.graph_endpoint,
            )
        } else {
            normalize_endpoint(
                ModelProvider::LlamaCppLocal,
                &settings.local_profile.graph_endpoint,
            )
        },
        embed_endpoint: if active_provider == ModelProvider::OpenAiCompatible {
            normalize_endpoint(
                ModelProvider::OpenAiCompatible,
                &settings.remote_profile.embed_endpoint,
            )
        } else {
            normalize_endpoint(
                ModelProvider::LlamaCppLocal,
                &settings.local_profile.embed_endpoint,
            )
        },
        api_key: if active_provider == ModelProvider::OpenAiCompatible {
            normalize_optional_text(settings.remote_profile.api_key.clone())
        } else {
            None
        },
        models_root: if active_provider == ModelProvider::LlamaCppLocal {
            normalize_optional_text(settings.local_profile.models_root.clone())
        } else {
            None
        },
        chat_model: if active_provider == ModelProvider::OpenAiCompatible {
            settings.remote_profile.chat_model.trim().to_string()
        } else {
            settings.local_profile.chat_model.trim().to_string()
        },
        graph_model: if active_provider == ModelProvider::OpenAiCompatible {
            settings.remote_profile.graph_model.trim().to_string()
        } else {
            settings.local_profile.graph_model.trim().to_string()
        },
        embed_model: if active_provider == ModelProvider::OpenAiCompatible {
            settings.remote_profile.embed_model.trim().to_string()
        } else {
            settings.local_profile.embed_model.trim().to_string()
        },
        chat_context_length: if active_provider == ModelProvider::OpenAiCompatible {
            settings.remote_profile.chat_context_length
        } else {
            settings.local_profile.chat_context_length
        },
        graph_context_length: if active_provider == ModelProvider::OpenAiCompatible {
            settings.remote_profile.graph_context_length
        } else {
            settings.local_profile.graph_context_length
        },
        embed_context_length: if active_provider == ModelProvider::OpenAiCompatible {
            settings.remote_profile.embed_context_length
        } else {
            settings.local_profile.embed_context_length
        },
        chat_concurrency: if active_provider == ModelProvider::OpenAiCompatible {
            settings.remote_profile.chat_concurrency
        } else {
            settings.local_profile.chat_concurrency
        },
        graph_concurrency: if active_provider == ModelProvider::OpenAiCompatible {
            settings.remote_profile.graph_concurrency
        } else {
            settings.local_profile.graph_concurrency
        },
        embed_concurrency: if active_provider == ModelProvider::OpenAiCompatible {
            settings.remote_profile.embed_concurrency
        } else {
            settings.local_profile.embed_concurrency
        },
    }
}

pub(crate) fn resolve_explicit_provider(settings: &AppSettings) -> Option<ModelProvider> {
    settings
        .active_provider
        .clone()
        .or_else(|| settings.provider.clone())
        .or_else(|| std::env::var(MEMORI_MODEL_PROVIDER_ENV).ok())
        .and_then(|value| {
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(ModelProvider::from_value(&trimmed))
            }
        })
}

pub(crate) fn resolve_configured_active_runtime_settings(
    settings: &AppSettings,
) -> Option<ActiveRuntimeModelSettings> {
    let explicit_provider = resolve_explicit_provider(settings)?;
    let mut model_settings = resolve_model_settings(settings);
    model_settings.active_provider = provider_to_string(explicit_provider);
    let active = resolve_active_runtime_settings(&model_settings);

    let configured = if explicit_provider == ModelProvider::OpenAiCompatible {
        !active.chat_endpoint.trim().is_empty()
            && !active.graph_endpoint.trim().is_empty()
            && !active.embed_endpoint.trim().is_empty()
            && active
                .api_key
                .as_deref()
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false)
            && !active.chat_model.trim().is_empty()
            && !active.graph_model.trim().is_empty()
            && !active.embed_model.trim().is_empty()
    } else {
        !active.chat_endpoint.trim().is_empty()
            && !active.graph_endpoint.trim().is_empty()
            && !active.embed_endpoint.trim().is_empty()
            && !active.chat_model.trim().is_empty()
            && !active.graph_model.trim().is_empty()
            && !active.embed_model.trim().is_empty()
    };

    if configured { Some(active) } else { None }
}

pub(crate) fn is_model_not_configured_message(message: &str) -> bool {
    message.contains(MODEL_NOT_CONFIGURED_CODE) || message.contains(MODEL_NOT_CONFIGURED_MESSAGE)
}

pub(crate) fn default_indexing_status(settings: &AppSettings) -> IndexingStatus {
    let indexing = resolve_indexing_config(settings);
    IndexingStatus {
        phase: "idle".to_string(),
        indexed_docs: 0,
        indexed_chunks: 0,
        graphed_chunks: 0,
        graph_backlog: 0,
        total_docs: 0,
        total_chunks: 0,
        progress_percent: 0,
        last_scan_at: None,
        last_error: None,
        paused: false,
        mode: indexing.mode,
        resource_budget: indexing.resource_budget,
        rebuild_state: "ready".to_string(),
        rebuild_reason: None,
        index_format_version: 0,
        parser_format_version: 0,
    }
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

pub(crate) fn normalize_optional_existing_file(
    value: Option<String>,
    label: &str,
) -> Result<Option<String>, String> {
    let Some(path) = normalize_optional_text(value) else {
        return Ok(None);
    };
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("{label} path does not exist: {}", p.display()));
    }
    if !p.is_file() {
        return Err(format!("{label} path is not a file: {}", p.display()));
    }
    Ok(Some(
        p.canonicalize().unwrap_or(p).to_string_lossy().to_string(),
    ))
}

pub(crate) fn normalize_endpoint(provider: ModelProvider, endpoint: &str) -> String {
    let trimmed = endpoint.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }
    if provider == ModelProvider::OpenAiCompatible {
        memori_core::DEFAULT_MODEL_ENDPOINT_OPENAI.to_string()
    } else {
        DEFAULT_CHAT_ENDPOINT.to_string()
    }
}

pub(crate) fn apply_model_settings_to_env(settings: ActiveRuntimeModelSettings) {
    // SAFETY: set_var affects process env; desktop runtime is single process and this is used as runtime config source.
    unsafe {
        std::env::set_var(
            MEMORI_MODEL_PROVIDER_ENV,
            provider_to_string(settings.provider),
        );
        std::env::set_var(MEMORI_MODEL_ENDPOINT_ENV, &settings.chat_endpoint);
        std::env::set_var(MEMORI_CHAT_ENDPOINT_ENV, &settings.chat_endpoint);
        std::env::set_var(MEMORI_GRAPH_ENDPOINT_ENV, &settings.graph_endpoint);
        std::env::set_var(MEMORI_EMBED_ENDPOINT_ENV, &settings.embed_endpoint);
        std::env::set_var(MEMORI_CHAT_MODEL_ENV, &settings.chat_model);
        std::env::set_var(MEMORI_GRAPH_MODEL_ENV, &settings.graph_model);
        std::env::set_var(MEMORI_EMBED_MODEL_ENV, &settings.embed_model);
        if let Some(key) = settings.api_key.as_ref() {
            std::env::set_var(MEMORI_MODEL_API_KEY_ENV, key);
        } else {
            std::env::remove_var(MEMORI_MODEL_API_KEY_ENV);
        }
        if let Some(v) = settings.chat_context_length {
            std::env::set_var("MEMORI_CHAT_CONTEXT_LENGTH", v.to_string());
        } else {
            std::env::remove_var("MEMORI_CHAT_CONTEXT_LENGTH");
        }
        if let Some(v) = settings.graph_context_length {
            std::env::set_var("MEMORI_GRAPH_CONTEXT_LENGTH", v.to_string());
        } else {
            std::env::remove_var("MEMORI_GRAPH_CONTEXT_LENGTH");
        }
        if let Some(v) = settings.embed_context_length {
            std::env::set_var("MEMORI_EMBED_CONTEXT_LENGTH", v.to_string());
        } else {
            std::env::remove_var("MEMORI_EMBED_CONTEXT_LENGTH");
        }
        if let Some(v) = settings.chat_concurrency {
            std::env::set_var("MEMORI_CHAT_CONCURRENCY", v.to_string());
        } else {
            std::env::remove_var("MEMORI_CHAT_CONCURRENCY");
        }
        if let Some(v) = settings.graph_concurrency {
            std::env::set_var("MEMORI_GRAPH_CONCURRENCY", v.to_string());
        } else {
            std::env::remove_var("MEMORI_GRAPH_CONCURRENCY");
        }
        if let Some(v) = settings.embed_concurrency {
            std::env::set_var("MEMORI_EMBED_CONCURRENCY", v.to_string());
        } else {
            std::env::remove_var("MEMORI_EMBED_CONCURRENCY");
        }
    }
}

pub(crate) fn resolve_indexing_config(settings: &AppSettings) -> IndexingConfig {
    let mode = settings
        .indexing_mode
        .as_deref()
        .map(IndexingMode::from_value)
        .unwrap_or(IndexingMode::Continuous);
    let resource_budget = settings
        .resource_budget
        .as_deref()
        .map(ResourceBudget::from_value)
        .unwrap_or(ResourceBudget::Low);
    let schedule_window = if mode == IndexingMode::Scheduled {
        Some(ScheduleWindow {
            start: settings
                .schedule_start
                .clone()
                .unwrap_or_else(|| "00:00".to_string()),
            end: settings
                .schedule_end
                .clone()
                .unwrap_or_else(|| "06:00".to_string()),
        })
    } else {
        None
    };

    IndexingConfig {
        mode,
        resource_budget,
        schedule_window,
    }
}

pub(crate) fn chrono_like_now_token() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
        .unwrap_or(0);
    now.to_string()
}
