use std::collections::HashSet;
use std::str::FromStr;

use crate::{
    DEFAULT_CHAT_ENDPOINT, DEFAULT_CHAT_MODEL_QWEN3, DEFAULT_EMBED_ENDPOINT,
    DEFAULT_EMBED_MODEL_QWEN3, DEFAULT_GRAPH_ENDPOINT, DEFAULT_GRAPH_MODEL_QWEN3,
    DEFAULT_RERANK_ENDPOINT, DEFAULT_RERANK_MODEL_BGE, MEMORI_CHAT_API_FORMAT_ENV,
    MEMORI_CHAT_ENDPOINT_ENV, MEMORI_CHAT_MODEL_ENV, MEMORI_EMBED_ENDPOINT_ENV,
    MEMORI_EMBED_MODEL_ENV, MEMORI_GRAPH_ENDPOINT_ENV, MEMORI_GRAPH_MODEL_ENV,
    MEMORI_MODEL_API_KEY_ENV, MEMORI_MODEL_PROTOCOL_ENV, MEMORI_MODEL_PROVIDER_ENV,
    MEMORI_RERANK_ENABLED_ENV, MEMORI_RERANK_ENDPOINT_ENV, MEMORI_RERANK_MODEL_ENV,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteModelProtocol {
    OpenAiChatCompletions,
    OpenAiResponses,
}

impl RemoteModelProtocol {
    pub fn from_value(text: &str) -> Self {
        match text.trim().to_ascii_lowercase().as_str() {
            "openai_responses" | "responses" | "response" => Self::OpenAiResponses,
            _ => Self::OpenAiChatCompletions,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenAiChatCompletions => "openai_chat_completions",
            Self::OpenAiResponses => "openai_responses",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatApiFormat {
    Chat,
    Responses,
}

impl ChatApiFormat {
    pub fn from_value(text: &str) -> Self {
        match text.trim().to_ascii_lowercase().as_str() {
            "responses" | "response" | "openai_responses" | "openai-response" => Self::Responses,
            _ => Self::Chat,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Responses => "responses",
        }
    }
}

pub fn build_openai_url(endpoint: &str, tail: &str) -> String {
    let host = endpoint.trim();
    let tail = tail.trim_start_matches('/');
    if let Some(stripped) = host.strip_suffix('#') {
        return stripped.to_string();
    }
    if host.ends_with('/') {
        return format!("{host}{tail}");
    }
    // 防御：endpoint 已显式带 /v1（无尾斜杠）时不再重复追加，
    // 避免拼成 /v1/v1/... 导致 404（对大小写不敏感）。
    if host.to_ascii_lowercase().ends_with("/v1") {
        return format!("{host}/{tail}");
    }
    format!("{}/v1/{}", host.trim_end_matches('/'), tail)
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
    pub protocol: RemoteModelProtocol,
    pub api_format: ChatApiFormat,
    pub chat_endpoint: String,
    pub chat_model: String,
    pub graph_endpoint: String,
    pub graph_model: String,
    pub embed_endpoint: String,
    pub embed_model: String,
    /// cross-encoder 重排服务（llama-server --reranking）。第 4 个本地角色，独立端口。
    pub rerank_endpoint: String,
    pub rerank_model: String,
    /// 是否启用召回后重排。关闭或服务不可达时检索自动回落到 RRF 排序。
    pub rerank_enabled: bool,
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
    let rerank_endpoint = std::env::var(MEMORI_RERANK_ENDPOINT_ENV)
        .unwrap_or_else(|_| DEFAULT_RERANK_ENDPOINT.to_string());
    let rerank_model = std::env::var(MEMORI_RERANK_MODEL_ENV)
        .unwrap_or_else(|_| DEFAULT_RERANK_MODEL_BGE.to_string());
    // 默认开启；服务不可达时检索层会自动降级，因此 opt-out 而非 opt-in。
    let rerank_enabled = std::env::var(MEMORI_RERANK_ENABLED_ENV)
        .ok()
        .map(|value| {
            !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "off" | "disabled" | "no"
            )
        })
        .unwrap_or(true);

    RuntimeModelConfig {
        provider,
        protocol: std::env::var(MEMORI_MODEL_PROTOCOL_ENV)
            .map(|v| RemoteModelProtocol::from_value(&v))
            .unwrap_or(RemoteModelProtocol::OpenAiChatCompletions),
        api_format: std::env::var(MEMORI_CHAT_API_FORMAT_ENV)
            .map(|v| ChatApiFormat::from_value(&v))
            .unwrap_or_else(|_| {
                std::env::var(MEMORI_MODEL_PROTOCOL_ENV)
                    .map(|v| match RemoteModelProtocol::from_value(&v) {
                        RemoteModelProtocol::OpenAiResponses => ChatApiFormat::Responses,
                        RemoteModelProtocol::OpenAiChatCompletions => ChatApiFormat::Chat,
                    })
                    .unwrap_or(ChatApiFormat::Chat)
            }),
        chat_endpoint,
        chat_model,
        graph_endpoint,
        graph_model,
        embed_endpoint,
        embed_model,
        rerank_endpoint,
        rerank_model,
        rerank_enabled,
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

/// 首次调用前 `MEMORI_OCR_TESSERACT_PATH` 是否已被外部显式设置。
///
/// 用于区分两种来源：外部预设的环境变量（优先级最高，永不覆盖）与本函数自己写入的值
/// （应随 settings 变化而更新）。若只判断"变量是否存在"，第二次调用会把自己上次写入的
/// 值误认为是外部设置，导致在设置里改路径必须重启应用才生效。
static OCR_ENV_PREEXISTING: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// 把 settings 里的 OCR tesseract 路径注入进程环境（供 parser 的 OCR 调用读取）。
/// 优先级：外部显式环境变量 > settings 配置；settings 为空/空白时清除注入值，
/// 让 parser 回退到 PATH 查找。
pub fn apply_ocr_path_to_env(configured: Option<&str>) {
    let preexisting = *OCR_ENV_PREEXISTING
        .get_or_init(|| std::env::var_os(memori_parser::OCR_TESSERACT_PATH_ENV).is_some());
    if preexisting {
        return;
    }
    let configured = configured.map(str::trim).filter(|value| !value.is_empty());
    unsafe {
        match configured {
            Some(path) => std::env::set_var(memori_parser::OCR_TESSERACT_PATH_ENV, path),
            None => std::env::remove_var(memori_parser::OCR_TESSERACT_PATH_ENV),
        }
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
    for (name, endpoint, model, enabled) in [
        (
            "chat",
            runtime.chat_endpoint.as_str(),
            runtime.chat_model.as_str(),
            true,
        ),
        (
            "graph",
            runtime.graph_endpoint.as_str(),
            runtime.graph_model.as_str(),
            true,
        ),
        (
            "embed",
            runtime.embed_endpoint.as_str(),
            runtime.embed_model.as_str(),
            true,
        ),
        (
            "rerank",
            runtime.rerank_endpoint.as_str(),
            runtime.rerank_model.as_str(),
            runtime.rerank_enabled,
        ),
    ] {
        if !enabled {
            continue;
        }
        if let Err(violation) =
            validate_provider_request(policy, runtime.provider, endpoint, &[model.to_string()])
        {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 注入优先级与边界（单函数内顺序执行：进程环境变量为全局状态，拆分测试会并行互扰）。
    #[test]
    fn apply_ocr_path_to_env_priority_and_bounds() {
        // 1) settings 有值 → 注入成功。
        unsafe {
            std::env::remove_var(memori_parser::OCR_TESSERACT_PATH_ENV);
        }
        apply_ocr_path_to_env(Some("D:/tesseract/tesseract.exe"));
        assert_eq!(
            std::env::var(memori_parser::OCR_TESSERACT_PATH_ENV).unwrap(),
            "D:/tesseract/tesseract.exe"
        );
        // 2) settings 改了路径 → 立即生效（改配置不需要重启应用）。
        apply_ocr_path_to_env(Some("D:/settings/path.exe"));
        assert_eq!(
            std::env::var(memori_parser::OCR_TESSERACT_PATH_ENV).unwrap(),
            "D:/settings/path.exe"
        );
        // 3) settings 为空/空白 → 清除注入值，让 parser 回退 PATH 查找。
        apply_ocr_path_to_env(None);
        assert!(std::env::var_os(memori_parser::OCR_TESSERACT_PATH_ENV).is_none());
        apply_ocr_path_to_env(Some("D:/settings/path.exe"));
        apply_ocr_path_to_env(Some("   "));
        assert!(std::env::var_os(memori_parser::OCR_TESSERACT_PATH_ENV).is_none());
    }

    #[test]
    fn build_openai_url_adds_v1_for_plain_host() {
        assert_eq!(
            build_openai_url("https://api.example.com", "chat/completions"),
            "https://api.example.com/v1/chat/completions"
        );
        assert_eq!(
            build_openai_url("https://api.example.com", "responses"),
            "https://api.example.com/v1/responses"
        );
        assert_eq!(
            build_openai_url("https://api.example.com", "embeddings"),
            "https://api.example.com/v1/embeddings"
        );
        assert_eq!(
            build_openai_url("https://api.example.com", "models"),
            "https://api.example.com/v1/models"
        );
    }

    #[test]
    fn build_openai_url_keeps_slash_versioned_host() {
        assert_eq!(
            build_openai_url("https://api.example.com/v1/", "chat/completions"),
            "https://api.example.com/v1/chat/completions"
        );
        assert_eq!(
            build_openai_url("https://api.example.com/custom/", "models"),
            "https://api.example.com/custom/models"
        );
    }

    /// endpoint 显式带 /v1 但**无尾斜杠**时不应再追加一层 /v1（曾实测拼成 /v1/v1 导致 404）。
    #[test]
    fn build_openai_url_does_not_double_v1_suffix() {
        assert_eq!(
            build_openai_url("https://api.example.com/v1", "chat/completions"),
            "https://api.example.com/v1/chat/completions"
        );
        assert_eq!(
            build_openai_url("http://localhost:11434/v1", "embeddings"),
            "http://localhost:11434/v1/embeddings"
        );
    }

    #[test]
    fn build_openai_url_hash_forces_exact_url() {
        assert_eq!(
            build_openai_url("https://api.example.com/custom#", "chat/completions"),
            "https://api.example.com/custom"
        );
        assert_eq!(
            build_openai_url("https://api.example.com/custom#", "responses"),
            "https://api.example.com/custom"
        );
    }

    fn sample_remote_runtime(rerank_enabled: bool, rerank_model: &str) -> RuntimeModelConfig {
        RuntimeModelConfig {
            provider: ModelProvider::OpenAiCompatible,
            protocol: RemoteModelProtocol::OpenAiChatCompletions,
            api_format: ChatApiFormat::Chat,
            chat_endpoint: "https://models.company.local/v1".to_string(),
            chat_model: "approved-chat".to_string(),
            graph_endpoint: "https://models.company.local/v1".to_string(),
            graph_model: "approved-chat".to_string(),
            embed_endpoint: "https://models.company.local/v1".to_string(),
            embed_model: "approved-embed".to_string(),
            rerank_endpoint: "https://models.company.local/v1".to_string(),
            rerank_model: rerank_model.to_string(),
            rerank_enabled,
            api_key: Some("secret".to_string()),
            chat_context_length: None,
            graph_context_length: None,
            embed_context_length: None,
            chat_concurrency: None,
            graph_concurrency: None,
            embed_concurrency: None,
        }
    }

    #[test]
    fn disabled_rerank_does_not_block_remote_runtime_allowlist() {
        let policy = EnterpriseModelPolicy {
            egress_mode: EgressMode::Allowlist,
            allowed_model_endpoints: vec!["https://models.company.local/v1".to_string()],
            allowed_models: vec!["approved-chat".to_string(), "approved-embed".to_string()],
        };

        let runtime = sample_remote_runtime(false, "gte-multilingual-reranker-base");
        assert!(validate_runtime_model_settings(&policy, &runtime).is_ok());
    }

    #[test]
    fn enabled_rerank_still_requires_allowlisted_model() {
        let policy = EnterpriseModelPolicy {
            egress_mode: EgressMode::Allowlist,
            allowed_model_endpoints: vec!["https://models.company.local/v1".to_string()],
            allowed_models: vec!["approved-chat".to_string(), "approved-embed".to_string()],
        };

        let runtime = sample_remote_runtime(true, "gte-multilingual-reranker-base");
        let violation = validate_runtime_model_settings(&policy, &runtime)
            .expect_err("enabled rerank should still be validated");
        assert_eq!(violation.code, "model_not_allowlisted");
        assert!(violation.message.contains("gte-multilingual-reranker-base"));
    }
}
