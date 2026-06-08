use crate::*;

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
    let local_rerank_endpoint = resolve_endpoint(
        settings.local_rerank_endpoint.clone(),
        None,
        None,
        MEMORI_RERANK_ENDPOINT_ENV,
        MEMORI_MODEL_ENDPOINT_ENV,
        DEFAULT_RERANK_ENDPOINT,
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

    let local_rerank_model = settings
        .local_rerank_model
        .clone()
        .or_else(|| {
            if env_provider == ModelProvider::LlamaCppLocal {
                std::env::var(MEMORI_RERANK_MODEL_ENV).ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| DEFAULT_RERANK_MODEL_BGE.to_string());

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
    let remote_api_format = settings
        .remote_api_format
        .clone()
        .or_else(|| {
            settings.remote_protocol.as_ref().map(|protocol| {
                if normalize_remote_protocol(Some(protocol)) == "openai_responses" {
                    "responses".to_string()
                } else {
                    "chat".to_string()
                }
            })
        })
        .unwrap_or_else(|| "chat".to_string());

    // 自愈：历史配置可能把三个本地角色塌缩到同一端口（共享的 local_endpoint 回退导致）。
    // 读取时检测冲突并把 graph/embed 重置为各自默认端口，避免“三个角色都指向 18001”。
    let (local_chat_endpoint, local_graph_endpoint, local_embed_endpoint, local_rerank_endpoint) =
        dedupe_local_endpoints(
            normalize_endpoint(ModelProvider::LlamaCppLocal, &local_chat_endpoint),
            normalize_endpoint(ModelProvider::LlamaCppLocal, &local_graph_endpoint),
            normalize_endpoint(ModelProvider::LlamaCppLocal, &local_embed_endpoint),
            normalize_endpoint(ModelProvider::LlamaCppLocal, &local_rerank_endpoint),
        );

    ModelSettingsDto {
        active_provider: provider_to_string(active_provider),
        local_profile: LocalModelProfileDto {
            chat_endpoint: local_chat_endpoint,
            graph_endpoint: local_graph_endpoint,
            embed_endpoint: local_embed_endpoint,
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
            rerank_endpoint: local_rerank_endpoint,
            rerank_model: local_rerank_model,
            rerank_model_path: normalize_optional_text(settings.local_rerank_model_path.clone()),
            rerank_context_length: settings.local_rerank_context_length,
            rerank_concurrency: settings.local_rerank_concurrency,
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
            api_format: Some(normalize_chat_api_format(Some(&remote_api_format))),
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
    // 空端口按角色填默认（18001/18002/18003），而不是统一回退到对话端口。
    let local_chat_endpoint =
        normalize_local_endpoint_for_role("chat", &payload.local_profile.chat_endpoint);
    let local_graph_endpoint =
        normalize_local_endpoint_for_role("graph", &payload.local_profile.graph_endpoint);
    let local_embed_endpoint =
        normalize_local_endpoint_for_role("embed", &payload.local_profile.embed_endpoint);
    let local_rerank_endpoint =
        normalize_local_endpoint_for_role("rerank", &payload.local_profile.rerank_endpoint);
    // 拒绝把多个角色配到同一端口（保存时硬校验，杜绝再次塌缩）。
    ensure_distinct_local_endpoints(
        &local_chat_endpoint,
        &local_graph_endpoint,
        &local_embed_endpoint,
        &local_rerank_endpoint,
    )?;
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
    // 重排模型可选：留空时回落到默认模型名，不报错（重排服务可不启用）。
    let local_rerank_model = {
        let trimmed = payload.local_profile.rerank_model.trim();
        if trimmed.is_empty() {
            DEFAULT_RERANK_MODEL_BGE.to_string()
        } else {
            trimmed.to_string()
        }
    };
    let remote_chat_model = payload.remote_profile.chat_model.trim().to_string();
    let remote_graph_model = payload.remote_profile.graph_model.trim().to_string();
    let remote_embed_model = payload.remote_profile.embed_model.trim().to_string();
    let remote_protocol = normalize_remote_protocol(Some(&payload.remote_profile.protocol));
    let remote_api_format = normalize_chat_api_format(payload.remote_profile.api_format.as_deref());

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
    let local_rerank_model_path =
        normalize_optional_existing_file(payload.local_profile.rerank_model_path, "rerank model")?;
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
            rerank_endpoint: local_rerank_endpoint,
            rerank_model: local_rerank_model,
            rerank_model_path: local_rerank_model_path,
            rerank_context_length: payload.local_profile.rerank_context_length,
            rerank_concurrency: payload.local_profile.rerank_concurrency,
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
            api_format: Some(remote_api_format),
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
        protocol: if active_provider == ModelProvider::OpenAiCompatible {
            settings.remote_profile.protocol.clone()
        } else {
            "openai_chat_completions".to_string()
        },
        api_format: if active_provider == ModelProvider::OpenAiCompatible {
            normalize_chat_api_format(settings.remote_profile.api_format.as_deref())
        } else {
            "chat".to_string()
        },
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
        // 重排为本地专属角色：仅本地 provider 启用，远程模式下关闭。
        rerank_endpoint: normalize_endpoint(
            ModelProvider::LlamaCppLocal,
            &settings.local_profile.rerank_endpoint,
        ),
        rerank_model: settings.local_profile.rerank_model.trim().to_string(),
        rerank_enabled: active_provider == ModelProvider::LlamaCppLocal,
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
        rebuild_state: "required".to_string(),
        rebuild_reason: Some("engine_unavailable".to_string()),
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

pub(crate) fn normalize_chat_api_format(value: Option<&str>) -> String {
    match value
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "responses" | "response" | "openai_responses" | "openai-response" => {
            "responses".to_string()
        }
        _ => "chat".to_string(),
    }
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
        if provider == ModelProvider::OpenAiCompatible {
            if trimmed.ends_with('/') || trimmed.ends_with('#') {
                return trimmed.to_string();
            }
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

/// 每个本地模型角色的默认 endpoint。三个角色的端口必须互不相同，
/// 否则一个 llama-server 进程无法同时服务多个角色（且 embed 需独立的 `--embedding` 服务）。
pub(crate) fn default_local_endpoint_for_role(role: &str) -> &'static str {
    match role {
        "graph" => DEFAULT_GRAPH_ENDPOINT,
        "embed" => DEFAULT_EMBED_ENDPOINT,
        "rerank" => DEFAULT_RERANK_ENDPOINT,
        _ => DEFAULT_CHAT_ENDPOINT,
    }
}

/// 保存时按角色归一化本地 endpoint：为空则填入该角色的默认端口，
/// 而不是统一回退到对话端口（历史 bug 会把三个角色塌缩到同一端口）。
pub(crate) fn normalize_local_endpoint_for_role(role: &str, endpoint: &str) -> String {
    let trimmed = endpoint.trim();
    if trimmed.is_empty() {
        default_local_endpoint_for_role(role).to_string()
    } else {
        trimmed.to_string()
    }
}

/// 解析 endpoint 的 (host, port)，用于判断两个角色是否落在同一个服务实例上。
pub(crate) fn endpoint_host_port(endpoint: &str) -> Option<(String, u16)> {
    let url = reqwest::Url::parse(endpoint.trim()).ok()?;
    let host = url.host_str()?.to_ascii_lowercase();
    let port = url.port_or_known_default()?;
    Some((host, port))
}

/// 两个 endpoint 是否指向同一个 host:port（无法解析时退化为字符串比较）。
fn same_endpoint_target(a: &str, b: &str) -> bool {
    match (endpoint_host_port(a), endpoint_host_port(b)) {
        (Some(x), Some(y)) => x == y,
        _ => a.trim() == b.trim(),
    }
}

/// 自愈：本地三角色端口塌缩到同一个时，把冲突的 graph/embed 重置为各自默认端口。
/// 用于读取可能已损坏的历史配置（例如三个角色都被写成 18001）。
pub(crate) fn dedupe_local_endpoints(
    chat: String,
    graph: String,
    embed: String,
    rerank: String,
) -> (String, String, String, String) {
    let graph = if same_endpoint_target(&chat, &graph) {
        default_local_endpoint_for_role("graph").to_string()
    } else {
        graph
    };
    let embed = if same_endpoint_target(&chat, &embed) || same_endpoint_target(&graph, &embed) {
        default_local_endpoint_for_role("embed").to_string()
    } else {
        embed
    };
    let rerank = if same_endpoint_target(&chat, &rerank)
        || same_endpoint_target(&graph, &rerank)
        || same_endpoint_target(&embed, &rerank)
    {
        default_local_endpoint_for_role("rerank").to_string()
    } else {
        rerank
    };
    (chat, graph, embed, rerank)
}

/// 校验本地四角色端口互不相同。保存时调用，拒绝把多个角色配到同一端口。
pub(crate) fn ensure_distinct_local_endpoints(
    chat: &str,
    graph: &str,
    embed: &str,
    rerank: &str,
) -> Result<(), String> {
    let endpoints = [chat, graph, embed, rerank];
    let collision = (0..endpoints.len()).any(|i| {
        ((i + 1)..endpoints.len()).any(|j| same_endpoint_target(endpoints[i], endpoints[j]))
    });
    if collision {
        return Err(
            "对话/图谱/向量/重排四个本地模型必须使用不同的端口：一个 llama-server 进程只能服务一个角色，向量模型需要 --embedding、重排模型需要 --reranking 独立服务。请为每个角色设置不同的端口（默认 18001 / 18002 / 18003 / 18004）。"
                .to_string(),
        );
    }
    Ok(())
}

pub(crate) fn apply_model_settings_to_env(settings: ActiveRuntimeModelSettings) {
    // SAFETY: set_var affects process env; desktop runtime is single process and this is used as runtime config source.
    unsafe {
        std::env::set_var(
            MEMORI_MODEL_PROVIDER_ENV,
            provider_to_string(settings.provider),
        );
        std::env::set_var(memori_core::MEMORI_MODEL_PROTOCOL_ENV, &settings.protocol);
        std::env::set_var(
            memori_core::MEMORI_CHAT_API_FORMAT_ENV,
            &settings.api_format,
        );
        std::env::set_var(MEMORI_MODEL_ENDPOINT_ENV, &settings.chat_endpoint);
        std::env::set_var(MEMORI_CHAT_ENDPOINT_ENV, &settings.chat_endpoint);
        std::env::set_var(MEMORI_GRAPH_ENDPOINT_ENV, &settings.graph_endpoint);
        std::env::set_var(MEMORI_EMBED_ENDPOINT_ENV, &settings.embed_endpoint);
        std::env::set_var(MEMORI_CHAT_MODEL_ENV, &settings.chat_model);
        std::env::set_var(MEMORI_GRAPH_MODEL_ENV, &settings.graph_model);
        std::env::set_var(MEMORI_EMBED_MODEL_ENV, &settings.embed_model);
        std::env::set_var(MEMORI_RERANK_ENDPOINT_ENV, &settings.rerank_endpoint);
        std::env::set_var(MEMORI_RERANK_MODEL_ENV, &settings.rerank_model);
        std::env::set_var(
            MEMORI_RERANK_ENABLED_ENV,
            if settings.rerank_enabled { "1" } else { "0" },
        );
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

#[cfg(test)]
mod endpoint_tests {
    use super::{
        dedupe_local_endpoints, ensure_distinct_local_endpoints, normalize_endpoint,
        normalize_local_endpoint_for_role,
    };
    use memori_core::ModelProvider;

    #[test]
    fn collapsed_local_endpoints_self_heal_to_role_defaults() {
        // 历史 bug：三个角色都被塌缩到 18001。读取时应自愈成各自默认端口。
        let (chat, graph, embed, rerank) = dedupe_local_endpoints(
            "http://localhost:18001".to_string(),
            "http://localhost:18001".to_string(),
            "http://localhost:18001".to_string(),
            "http://localhost:18001".to_string(),
        );
        assert_eq!(chat, "http://localhost:18001");
        assert_eq!(graph, "http://localhost:18002");
        assert_eq!(embed, "http://localhost:18003");
        assert_eq!(rerank, "http://localhost:18004");
    }

    #[test]
    fn distinct_local_endpoints_are_left_untouched() {
        let (chat, graph, embed, rerank) = dedupe_local_endpoints(
            "http://localhost:18001".to_string(),
            "http://localhost:18002".to_string(),
            "http://localhost:18003".to_string(),
            "http://localhost:18004".to_string(),
        );
        assert_eq!(
            (
                chat.as_str(),
                graph.as_str(),
                embed.as_str(),
                rerank.as_str()
            ),
            (
                "http://localhost:18001",
                "http://localhost:18002",
                "http://localhost:18003",
                "http://localhost:18004",
            )
        );
    }

    #[test]
    fn empty_local_endpoint_falls_back_to_role_default_not_chat() {
        assert_eq!(
            normalize_local_endpoint_for_role("graph", "   "),
            "http://localhost:18002"
        );
        assert_eq!(
            normalize_local_endpoint_for_role("embed", ""),
            "http://localhost:18003"
        );
        assert_eq!(
            normalize_local_endpoint_for_role("rerank", ""),
            "http://localhost:18004"
        );
    }

    #[test]
    fn saving_same_port_for_multiple_roles_is_rejected() {
        let err = ensure_distinct_local_endpoints(
            "http://localhost:18001",
            "http://localhost:18001",
            "http://localhost:18003",
            "http://localhost:18004",
        )
        .unwrap_err();
        assert!(err.contains("不同的端口"));
        assert!(
            ensure_distinct_local_endpoints(
                "http://localhost:18001",
                "http://localhost:18002",
                "http://localhost:18003",
                "http://localhost:18004",
            )
            .is_ok()
        );
    }

    #[test]
    fn openai_compatible_endpoint_strips_full_request_path() {
        assert_eq!(
            normalize_endpoint(
                ModelProvider::OpenAiCompatible,
                "https://api.example.com/v1"
            ),
            "https://api.example.com"
        );
        assert_eq!(
            normalize_endpoint(
                ModelProvider::OpenAiCompatible,
                "https://api.example.com/v1/models"
            ),
            "https://api.example.com"
        );
        assert_eq!(
            normalize_endpoint(
                ModelProvider::OpenAiCompatible,
                "https://api.example.com/v1/chat/completions"
            ),
            "https://api.example.com"
        );
    }
}
