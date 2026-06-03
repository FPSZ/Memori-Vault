use super::*;

fn local_profile() -> LocalModelProfileDto {
    LocalModelProfileDto {
        endpoint: "http://localhost:18001".to_string(),
        models_root: None,
        chat_model: "local-chat".to_string(),
        graph_model: "local-graph".to_string(),
        embed_model: "local-embed".to_string(),
        chat_context_length: None,
        graph_context_length: None,
        embed_context_length: None,
        chat_concurrency: None,
        graph_concurrency: None,
        embed_concurrency: None,
        performance_preset: None,
        n_gpu_layers: None,
        batch_size: None,
        ubatch_size: None,
        threads: None,
        threads_batch: None,
        flash_attn: None,
        cache_type_k: None,
        cache_type_v: None,
    }
}

#[test]
fn remote_runtime_uses_role_specific_endpoints() {
    let settings = ModelSettingsDto {
        active_provider: "openai_compatible".to_string(),
        local_profile: local_profile(),
        remote_profile: RemoteModelProfileDto {
            protocol: "openai_chat_completions".to_string(),
            chat_endpoint: "https://chat.example.com".to_string(),
            graph_endpoint: "https://graph.example.com".to_string(),
            embed_endpoint: "https://embed.example.com".to_string(),
            endpoint: None,
            api_key: Some("sk-test".to_string()),
            chat_model: "remote-chat".to_string(),
            graph_model: "remote-graph".to_string(),
            embed_model: "remote-embed".to_string(),
            chat_context_length: None,
            graph_context_length: None,
            embed_context_length: None,
            chat_concurrency: None,
            graph_concurrency: None,
            embed_concurrency: None,
        },
        stop_local_models_on_exit: true,
    };

    let runtime = resolve_active_runtime_settings(&settings);

    assert_eq!(runtime.provider, ModelProvider::OpenAiCompatible);
    assert_eq!(runtime.chat_endpoint, "https://chat.example.com");
    assert_eq!(runtime.graph_endpoint, "https://graph.example.com");
    assert_eq!(runtime.embed_endpoint, "https://embed.example.com");
    assert_eq!(runtime.chat_model, "remote-chat");
    assert_eq!(runtime.graph_model, "remote-graph");
    assert_eq!(runtime.embed_model, "remote-embed");
}

#[test]
fn legacy_remote_endpoint_payload_expands_to_all_roles() {
    let payload = ModelSettingsDto {
        active_provider: "openai_compatible".to_string(),
        local_profile: local_profile(),
        remote_profile: RemoteModelProfileDto {
            protocol: "openai_chat_completions".to_string(),
            chat_endpoint: String::new(),
            graph_endpoint: String::new(),
            embed_endpoint: String::new(),
            endpoint: Some("https://legacy.example.com".to_string()),
            api_key: None,
            chat_model: "remote-chat".to_string(),
            graph_model: "remote-graph".to_string(),
            embed_model: "remote-embed".to_string(),
            chat_context_length: None,
            graph_context_length: None,
            embed_context_length: None,
            chat_concurrency: None,
            graph_concurrency: None,
            embed_concurrency: None,
        },
        stop_local_models_on_exit: true,
    };

    let normalized = normalize_model_settings_payload(payload).expect("payload normalizes");

    assert_eq!(
        normalized.remote_profile.chat_endpoint,
        "https://legacy.example.com"
    );
    assert_eq!(
        normalized.remote_profile.graph_endpoint,
        "https://legacy.example.com"
    );
    assert_eq!(
        normalized.remote_profile.embed_endpoint,
        "https://legacy.example.com"
    );
    assert!(normalized.remote_profile.endpoint.is_none());
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
            "https://api.example.com/v1/embeddings"
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
