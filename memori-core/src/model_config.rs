use std::collections::HashSet;
use std::str::FromStr;

use crate::{
    DEFAULT_CHAT_ENDPOINT, DEFAULT_CHAT_MODEL_QWEN3, DEFAULT_EMBED_ENDPOINT,
    DEFAULT_EMBED_MODEL_QWEN3, DEFAULT_GRAPH_ENDPOINT, DEFAULT_GRAPH_MODEL_QWEN3,
    MEMORI_CHAT_ENDPOINT_ENV, MEMORI_CHAT_MODEL_ENV, MEMORI_EMBED_ENDPOINT_ENV,
    MEMORI_EMBED_MODEL_ENV, MEMORI_GRAPH_ENDPOINT_ENV, MEMORI_GRAPH_MODEL_ENV,
    MEMORI_MODEL_API_KEY_ENV, MEMORI_MODEL_PROVIDER_ENV,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelProvider {
    LlamaCppLocal,
    OpenAiCompatible,
}

impl ModelProvider {
    pub fn from_value(text: &str) -> Self {
        text.parse().unwrap_or(Self::LlamaCppLocal)
    }
}

impl FromStr for ModelProvider {
    type Err = &'static str;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text.trim().to_ascii_lowercase().as_str() {
            "llama_cpp_local" | "llamacpp_local" | "llama.cpp_local" | "ollama_local" => {
                Ok(Self::LlamaCppLocal)
            }
            "openai_compatible" => Ok(Self::OpenAiCompatible),
            _ => Err("unknown model provider"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EgressMode {
    #[default]
    LocalOnly,
    Allowlist,
}

impl EgressMode {
    pub fn from_value(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "allowlist" => Self::Allowlist,
            _ => Self::LocalOnly,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct EnterpriseModelPolicy {
    pub egress_mode: EgressMode,
    pub allowed_model_endpoints: Vec<String>,
    pub allowed_models: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PolicyViolation {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct RuntimeModelConfig {
    pub provider: ModelProvider,
    pub chat_endpoint: String,
    pub chat_model: String,
    pub graph_endpoint: String,
    pub graph_model: String,
    pub embed_endpoint: String,
    pub embed_model: String,
    pub api_key: Option<String>,
    pub chat_context_length: Option<u32>,
    pub graph_context_length: Option<u32>,
    pub embed_context_length: Option<u32>,
    pub chat_concurrency: Option<u32>,
    pub graph_concurrency: Option<u32>,
    pub embed_concurrency: Option<u32>,
}

pub fn resolve_runtime_model_config_from_env() -> RuntimeModelConfig {
    let provider = std::env::var(MEMORI_MODEL_PROVIDER_ENV)
        .map(|v| ModelProvider::from_value(&v))
        .unwrap_or(ModelProvider::LlamaCppLocal);

    let chat_endpoint = std::env::var(MEMORI_CHAT_ENDPOINT_ENV)
        .unwrap_or_else(|_| DEFAULT_CHAT_ENDPOINT.to_string());
    let graph_endpoint = std::env::var(MEMORI_GRAPH_ENDPOINT_ENV)
        .unwrap_or_else(|_| DEFAULT_GRAPH_ENDPOINT.to_string());
    let embed_endpoint = std::env::var(MEMORI_EMBED_ENDPOINT_ENV)
        .unwrap_or_else(|_| DEFAULT_EMBED_ENDPOINT.to_string());

    let api_key = std::env::var(MEMORI_MODEL_API_KEY_ENV).ok().and_then(|v| {
        let trimmed = v.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });

    let chat_model = std::env::var(MEMORI_CHAT_MODEL_ENV)
        .unwrap_or_else(|_| DEFAULT_CHAT_MODEL_QWEN3.to_string());
    let graph_model = std::env::var(MEMORI_GRAPH_MODEL_ENV)
        .unwrap_or_else(|_| DEFAULT_GRAPH_MODEL_QWEN3.to_string());
    let embed_model = std::env::var(MEMORI_EMBED_MODEL_ENV)
        .unwrap_or_else(|_| DEFAULT_EMBED_MODEL_QWEN3.to_string());

    RuntimeModelConfig {
        provider,
        chat_endpoint,
        chat_model,
        graph_endpoint,
        graph_model,
        embed_endpoint,
        embed_model,
        api_key,
        chat_context_length: std::env::var("MEMORI_CHAT_CONTEXT_LENGTH")
            .ok()
            .and_then(|v| v.parse().ok()),
        graph_context_length: std::env::var("MEMORI_GRAPH_CONTEXT_LENGTH")
            .ok()
            .and_then(|v| v.parse().ok()),
        embed_context_length: std::env::var("MEMORI_EMBED_CONTEXT_LENGTH")
            .ok()
            .and_then(|v| v.parse().ok()),
        chat_concurrency: std::env::var("MEMORI_CHAT_CONCURRENCY")
            .ok()
            .and_then(|v| v.parse().ok()),
        graph_concurrency: std::env::var("MEMORI_GRAPH_CONCURRENCY")
            .ok()
            .and_then(|v| v.parse().ok()),
        embed_concurrency: std::env::var("MEMORI_EMBED_CONCURRENCY")
            .ok()
            .and_then(|v| v.parse().ok()),
    }
}

pub fn normalize_policy_endpoint(endpoint: &str) -> String {
    let trimmed = endpoint.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if let Ok(url) = reqwest::Url::parse(trimmed) {
        let host = url.host_str().map(|value| value.to_ascii_lowercase());
        let mut normalized = format!("{}://", url.scheme().to_ascii_lowercase());
        if let Some(host) = host {
            normalized.push_str(&host);
        } else {
            return trimmed.trim_end_matches('/').to_ascii_lowercase();
        }
        if let Some(port) = url.port() {
            normalized.push(':');
            normalized.push_str(&port.to_string());
        }
        let path = url.path().trim_end_matches('/');
        if !path.is_empty() && path != "/" {
            normalized.push_str(path);
        }
        return normalized;
    }

    trimmed.trim_end_matches('/').to_ascii_lowercase()
}

pub fn validate_provider_request(
    policy: &EnterpriseModelPolicy,
    provider: ModelProvider,
    endpoint: &str,
    models: &[String],
) -> Result<(), PolicyViolation> {
    if provider == ModelProvider::LlamaCppLocal {
        return Ok(());
    }

    if policy.egress_mode == EgressMode::LocalOnly {
        return Err(PolicyViolation {
            code: "policy_violation".to_string(),
            message: "Remote model endpoint blocked by enterprise policy".to_string(),
        });
    }

    let normalized_endpoint = normalize_policy_endpoint(endpoint);
    let normalized_allowlist = policy
        .allowed_model_endpoints
        .iter()
        .map(|item| normalize_policy_endpoint(item))
        .filter(|item| !item.is_empty())
        .collect::<HashSet<_>>();
    if !normalized_allowlist.is_empty() && !normalized_allowlist.contains(&normalized_endpoint) {
        return Err(PolicyViolation {
            code: "remote_endpoint_not_allowlisted".to_string(),
            message: format!(
                "Remote model endpoint blocked by enterprise policy: {}",
                endpoint.trim()
            ),
        });
    }

    let normalized_models = policy
        .allowed_models
        .iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect::<HashSet<_>>();
    if !normalized_models.is_empty() {
        for model in models {
            let trimmed = model.trim();
            if trimmed.is_empty() {
                continue;
            }
            if !normalized_models.contains(trimmed) {
                return Err(PolicyViolation {
                    code: "model_not_allowlisted".to_string(),
                    message: format!("Remote model blocked by enterprise policy: {trimmed}"),
                });
            }
        }
    }

    Ok(())
}

pub fn validate_runtime_model_settings(
    policy: &EnterpriseModelPolicy,
    runtime: &RuntimeModelConfig,
) -> Result<(), PolicyViolation> {
    for (name, endpoint) in [
        ("chat", &runtime.chat_endpoint),
        ("graph", &runtime.graph_endpoint),
        ("embed", &runtime.embed_endpoint),
    ] {
        if let Err(violation) = validate_provider_request(
            policy,
            runtime.provider,
            endpoint,
            &[
                runtime.chat_model.clone(),
                runtime.graph_model.clone(),
                runtime.embed_model.clone(),
            ],
        ) {
            return Err(match violation.code.as_str() {
                "policy_violation" => PolicyViolation {
                    code: "runtime_blocked_by_policy".to_string(),
                    message: format!(
                        "Runtime model configuration rejected for {name} endpoint before startup"
                    ),
                },
                _ => violation,
            });
        }
    }
    Ok(())
}
