use crate::*;
pub(crate) struct ActiveRuntimeModelSettings {
    pub(crate) provider: ModelProvider,
    pub(crate) protocol: String,
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
        protocol: memori_core::RemoteModelProtocol::from_value(&settings.protocol),
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
            ) == ModelProvider::LlamaCppLocal
            {
                settings.endpoint.clone()
            } else {
                None
            }
        })
        .or_else(|| {
            if env_provider == ModelProvider::LlamaCppLocal {
                std::env::var(MEMORI_MODEL_ENDPOINT_ENV).ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| DEFAULT_CHAT_ENDPOINT.to_string());

    let remote_chat_endpoint = settings
        .remote_chat_endpoint
        .clone()
        .or_else(|| settings.remote_endpoint.clone())
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
                std::env::var(MEMORI_CHAT_ENDPOINT_ENV)
                    .or_else(|_| std::env::var(MEMORI_MODEL_ENDPOINT_ENV))
                    .ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| memori_core::DEFAULT_MODEL_ENDPOINT_OPENAI.to_string());
    let remote_graph_endpoint = settings
        .remote_graph_endpoint
        .clone()
        .or_else(|| settings.remote_endpoint.clone())
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
                std::env::var(MEMORI_GRAPH_ENDPOINT_ENV)
                    .or_else(|_| std::env::var(MEMORI_MODEL_ENDPOINT_ENV))
                    .ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| memori_core::DEFAULT_MODEL_ENDPOINT_OPENAI.to_string());
    let remote_embed_endpoint = settings
        .remote_embed_endpoint
        .clone()
        .or_else(|| settings.remote_endpoint.clone())
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
                std::env::var(MEMORI_EMBED_ENDPOINT_ENV)
                    .or_else(|_| std::env::var(MEMORI_MODEL_ENDPOINT_ENV))
                    .ok()
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
            endpoint: normalize_endpoint(ModelProvider::LlamaCppLocal, &local_endpoint),
            models_root: normalize_optional_text(settings.local_models_root.clone()),
            chat_model: local_chat_model,
            graph_model: local_graph_model,
            embed_model: local_embed_model,
            chat_context_length: settings.local_chat_context_length,
            graph_context_length: settings.local_graph_context_length,
            embed_context_length: settings.local_embed_context_length,
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
            protocol: normalize_remote_protocol(settings.remote_protocol.as_deref()),
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
            endpoint: None,
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
    let local_endpoint = normalize_endpoint(
        ModelProvider::LlamaCppLocal,
        &payload.local_profile.endpoint,
    );
    let legacy_remote_endpoint = payload.remote_profile.endpoint.as_deref().unwrap_or("");
    let remote_chat_endpoint = normalize_endpoint(
        ModelProvider::OpenAiCompatible,
        if payload.remote_profile.chat_endpoint.trim().is_empty() {
            legacy_remote_endpoint
        } else {
            &payload.remote_profile.chat_endpoint
        },
    );
    let remote_graph_endpoint = normalize_endpoint(
        ModelProvider::OpenAiCompatible,
        if payload.remote_profile.graph_endpoint.trim().is_empty() {
            legacy_remote_endpoint
        } else {
            &payload.remote_profile.graph_endpoint
        },
    );
    let remote_embed_endpoint = normalize_endpoint(
        ModelProvider::OpenAiCompatible,
        if payload.remote_profile.embed_endpoint.trim().is_empty() {
            legacy_remote_endpoint
        } else {
            &payload.remote_profile.embed_endpoint
        },
    );

    let local_chat_model = payload.local_profile.chat_model.trim().to_string();
    let local_graph_model = payload.local_profile.graph_model.trim().to_string();
    let local_embed_model = payload.local_profile.embed_model.trim().to_string();
    let remote_chat_model = payload.remote_profile.chat_model.trim().to_string();
    let remote_graph_model = payload.remote_profile.graph_model.trim().to_string();
    let remote_embed_model = payload.remote_profile.embed_model.trim().to_string();
    let remote_protocol = normalize_remote_protocol(Some(&payload.remote_profile.protocol));

    if local_chat_model.is_empty()
        || local_graph_model.is_empty()
        || local_embed_model.is_empty()
        || remote_chat_model.is_empty()
        || remote_graph_model.is_empty()
        || remote_embed_model.is_empty()
    {
        return Err("chat/graph/embed model names cannot be empty".to_string());
    }

    let local_models_root =
        normalize_optional_text(payload.local_profile.models_root).map(|path| {
            let p = PathBuf::from(&path);
            p.canonicalize().unwrap_or(p).to_string_lossy().to_string()
        });
    let local_performance_preset =
        normalize_performance_preset(payload.local_profile.performance_preset);
    let local_cache_type_k = normalize_optional_text(payload.local_profile.cache_type_k);
    let local_cache_type_v = normalize_optional_text(payload.local_profile.cache_type_v);

    Ok(ModelSettingsDto {
        active_provider: provider_to_string(active_provider),
        local_profile: LocalModelProfileDto {
            endpoint: local_endpoint,
            models_root: local_models_root,
            chat_model: local_chat_model,
            graph_model: local_graph_model,
            embed_model: local_embed_model,
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
            protocol: remote_protocol,
            chat_endpoint: remote_chat_endpoint,
            graph_endpoint: remote_graph_endpoint,
            embed_endpoint: remote_embed_endpoint,
            endpoint: None,
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

pub(crate) fn normalize_remote_protocol(value: Option<&str>) -> String {
    match value
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "openai_responses" => "openai_responses".to_string(),
        _ => "openai_chat_completions".to_string(),
    }
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

pub(crate) fn normalize_endpoint(provider: ModelProvider, endpoint: &str) -> String {
    let trimmed = endpoint.trim();
    if !trimmed.is_empty() {
        if provider == ModelProvider::OpenAiCompatible {
            return strip_openai_compatible_request_path(trimmed);
        }
        return trimmed.to_string();
    }
    if provider == ModelProvider::OpenAiCompatible {
        memori_core::DEFAULT_MODEL_ENDPOINT_OPENAI.to_string()
    } else {
        DEFAULT_CHAT_ENDPOINT.to_string()
    }
}

pub(crate) fn strip_openai_compatible_request_path(endpoint: &str) -> String {
    let mut value = endpoint.trim().trim_end_matches('/').to_string();
    for suffix in [
        "/v1/chat/completions",
        "/v1/responses",
        "/v1/embeddings",
        "/v1/models",
        "/v1",
    ] {
        if value.to_ascii_lowercase().ends_with(suffix) {
            let keep = value.len() - suffix.len();
            value.truncate(keep);
            break;
        }
    }
    value
}

pub(crate) fn resolve_active_runtime_settings(
    settings: &ModelSettingsDto,
) -> ActiveRuntimeModelSettings {
    let active_provider = ModelProvider::from_value(&settings.active_provider);
    ActiveRuntimeModelSettings {
        provider: active_provider,
        protocol: if active_provider == ModelProvider::OpenAiCompatible {
            settings.remote_profile.protocol.clone()
        } else {
            "openai_chat_completions".to_string()
        },
        chat_endpoint: if active_provider == ModelProvider::OpenAiCompatible {
            normalize_endpoint(
                ModelProvider::OpenAiCompatible,
                &settings.remote_profile.chat_endpoint,
            )
        } else {
            normalize_endpoint(
                ModelProvider::LlamaCppLocal,
                &settings.local_profile.endpoint,
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
                &settings.local_profile.endpoint,
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
                &settings.local_profile.endpoint,
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

pub(crate) fn apply_model_settings_to_env(settings: ActiveRuntimeModelSettings) {
    // SAFETY: process-global config source for memori-core runtime.
    unsafe {
        std::env::set_var(
            MEMORI_MODEL_PROVIDER_ENV,
            provider_to_string(settings.provider),
        );
        std::env::set_var(memori_core::MEMORI_MODEL_PROTOCOL_ENV, &settings.protocol);
        // Legacy endpoint compatibility: expose the chat endpoint as the single endpoint.
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
