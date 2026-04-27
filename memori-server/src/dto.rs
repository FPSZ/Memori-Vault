use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use memori_core::EgressMode;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct AppSettings {
    pub(crate) watch_root: Option<String>,
    pub(crate) language: Option<String>,
    pub(crate) indexing_mode: Option<String>,
    pub(crate) resource_budget: Option<String>,
    pub(crate) schedule_start: Option<String>,
    pub(crate) schedule_end: Option<String>,
    pub(crate) active_provider: Option<String>,
    pub(crate) local_endpoint: Option<String>,
    pub(crate) local_models_root: Option<String>,
    pub(crate) local_chat_model: Option<String>,
    pub(crate) local_graph_model: Option<String>,
    pub(crate) local_embed_model: Option<String>,
    pub(crate) local_chat_context_length: Option<u32>,
    pub(crate) local_graph_context_length: Option<u32>,
    pub(crate) local_embed_context_length: Option<u32>,
    pub(crate) local_chat_concurrency: Option<u32>,
    pub(crate) local_graph_concurrency: Option<u32>,
    pub(crate) local_embed_concurrency: Option<u32>,
    pub(crate) remote_endpoint: Option<String>,
    pub(crate) remote_api_key: Option<String>,
    pub(crate) remote_chat_model: Option<String>,
    pub(crate) remote_graph_model: Option<String>,
    pub(crate) remote_embed_model: Option<String>,
    pub(crate) remote_chat_context_length: Option<u32>,
    pub(crate) remote_graph_context_length: Option<u32>,
    pub(crate) remote_embed_context_length: Option<u32>,
    pub(crate) remote_chat_concurrency: Option<u32>,
    pub(crate) remote_graph_concurrency: Option<u32>,
    pub(crate) remote_embed_concurrency: Option<u32>,
    pub(crate) enterprise_egress_mode: Option<String>,
    pub(crate) enterprise_allowed_model_endpoints: Option<Vec<String>>,
    pub(crate) enterprise_allowed_models: Option<Vec<String>>,
    pub(crate) oidc_issuer: Option<String>,
    pub(crate) oidc_client_id: Option<String>,
    pub(crate) oidc_redirect_uri: Option<String>,
    pub(crate) oidc_roles_claim: Option<String>,
    pub(crate) mcp_enabled: Option<bool>,
    pub(crate) mcp_transports: Option<Vec<String>>,
    pub(crate) mcp_http_bind: Option<String>,
    pub(crate) mcp_http_port: Option<u16>,
    pub(crate) mcp_access_mode: Option<String>,
    pub(crate) mcp_audit_enabled: Option<bool>,
    pub(crate) conversation_memory_enabled: Option<bool>,
    pub(crate) auto_memory_write: Option<String>,
    pub(crate) memory_write_requires_source: Option<bool>,
    pub(crate) memory_markdown_export_enabled: Option<bool>,
    pub(crate) default_context_budget: Option<String>,
    pub(crate) complex_context_budget: Option<String>,
    pub(crate) graph_ranking_enabled: Option<bool>,
    // legacy fields for backwards compatibility
    pub(crate) provider: Option<String>,
    pub(crate) endpoint: Option<String>,
    pub(crate) api_key: Option<String>,
    pub(crate) chat_model: Option<String>,
    pub(crate) graph_model: Option<String>,
    pub(crate) embed_model: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Role {
    Viewer,
    User,
    Operator,
    Admin,
}

impl Role {
    pub(crate) fn from_value(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "admin" => Self::Admin,
            "operator" => Self::Operator,
            "viewer" => Self::Viewer,
            _ => Self::User,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AuthConfigDto {
    pub(crate) issuer: String,
    pub(crate) client_id: String,
    pub(crate) redirect_uri: String,
    pub(crate) roles_claim: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct EnterprisePolicyDto {
    pub(crate) egress_mode: EgressMode,
    pub(crate) allowed_model_endpoints: Vec<String>,
    pub(crate) allowed_models: Vec<String>,
    pub(crate) indexing_default_mode: String,
    pub(crate) resource_budget_default: String,
    pub(crate) auth: AuthConfigDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AuditEventDto {
    pub(crate) actor: String,
    pub(crate) action: String,
    pub(crate) resource: String,
    pub(crate) timestamp: i64,
    pub(crate) result: String,
    pub(crate) metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SessionDto {
    pub(crate) subject: String,
    pub(crate) role: Role,
    pub(crate) issued_at: i64,
    pub(crate) expires_at: i64,
}

#[derive(Debug, Serialize)]
pub(crate) struct ServerMetricsDto {
    pub(crate) total_requests: u64,
    pub(crate) failed_requests: u64,
    pub(crate) ask_requests: u64,
    pub(crate) ask_failed: u64,
    pub(crate) ask_latency_avg_ms: f64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AppSettingsDto {
    pub(crate) watch_root: String,
    pub(crate) language: Option<String>,
    pub(crate) indexing_mode: String,
    pub(crate) resource_budget: String,
    pub(crate) schedule_start: Option<String>,
    pub(crate) schedule_end: Option<String>,
    pub(crate) conversation_memory_enabled: bool,
    pub(crate) auto_memory_write: String,
    pub(crate) memory_write_requires_source: bool,
    pub(crate) memory_markdown_export_enabled: bool,
    pub(crate) default_context_budget: String,
    pub(crate) complex_context_budget: String,
    pub(crate) graph_ranking_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MemorySettingsDto {
    pub(crate) conversation_memory_enabled: bool,
    pub(crate) auto_memory_write: String,
    pub(crate) memory_write_requires_source: bool,
    pub(crate) memory_markdown_export_enabled: bool,
    pub(crate) default_context_budget: String,
    pub(crate) complex_context_budget: String,
    pub(crate) graph_ranking_enabled: bool,
}

impl AppSettingsDto {
    pub(crate) fn from_settings(
        settings: AppSettings,
        watch_root: String,
        indexing: memori_core::IndexingConfig,
    ) -> Self {
        Self {
            watch_root,
            language: settings.language,
            indexing_mode: indexing.mode.as_str().to_string(),
            resource_budget: indexing.resource_budget.as_str().to_string(),
            schedule_start: indexing.schedule_window.as_ref().map(|w| w.start.clone()),
            schedule_end: indexing.schedule_window.as_ref().map(|w| w.end.clone()),
            conversation_memory_enabled: settings.conversation_memory_enabled.unwrap_or(true),
            auto_memory_write: settings
                .auto_memory_write
                .unwrap_or_else(|| "suggest".to_string()),
            memory_write_requires_source: settings.memory_write_requires_source.unwrap_or(true),
            memory_markdown_export_enabled: settings
                .memory_markdown_export_enabled
                .unwrap_or(false),
            default_context_budget: settings
                .default_context_budget
                .unwrap_or_else(|| "16k".to_string()),
            complex_context_budget: settings
                .complex_context_budget
                .unwrap_or_else(|| "32k".to_string()),
            graph_ranking_enabled: settings.graph_ranking_enabled.unwrap_or(false),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SetIndexingModePayload {
    pub(crate) indexing_mode: String,
    pub(crate) resource_budget: String,
    pub(crate) schedule_start: Option<String>,
    pub(crate) schedule_end: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LocalModelProfileDto {
    pub(crate) endpoint: String,
    pub(crate) models_root: Option<String>,
    pub(crate) chat_model: String,
    pub(crate) graph_model: String,
    pub(crate) embed_model: String,
    #[serde(default)]
    pub(crate) chat_context_length: Option<u32>,
    #[serde(default)]
    pub(crate) graph_context_length: Option<u32>,
    #[serde(default)]
    pub(crate) embed_context_length: Option<u32>,
    #[serde(default)]
    pub(crate) chat_concurrency: Option<u32>,
    #[serde(default)]
    pub(crate) graph_concurrency: Option<u32>,
    #[serde(default)]
    pub(crate) embed_concurrency: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RemoteModelProfileDto {
    pub(crate) endpoint: String,
    pub(crate) api_key: Option<String>,
    pub(crate) chat_model: String,
    pub(crate) graph_model: String,
    pub(crate) embed_model: String,
    #[serde(default)]
    pub(crate) chat_context_length: Option<u32>,
    #[serde(default)]
    pub(crate) graph_context_length: Option<u32>,
    #[serde(default)]
    pub(crate) embed_context_length: Option<u32>,
    #[serde(default)]
    pub(crate) chat_concurrency: Option<u32>,
    #[serde(default)]
    pub(crate) graph_concurrency: Option<u32>,
    #[serde(default)]
    pub(crate) embed_concurrency: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ModelSettingsDto {
    pub(crate) active_provider: String,
    pub(crate) local_profile: LocalModelProfileDto,
    pub(crate) remote_profile: RemoteModelProfileDto,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ModelErrorItem {
    pub(crate) code: String,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ModelAvailabilityDto {
    pub(crate) reachable: bool,
    pub(crate) models: Vec<String>,
    pub(crate) missing_roles: Vec<String>,
    pub(crate) errors: Vec<ModelErrorItem>,
    pub(crate) checked_provider: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ProviderModelsDto {
    pub(crate) from_folder: Vec<String>,
    pub(crate) from_service: Vec<String>,
    pub(crate) merged: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ProviderModelFetchError {
    pub(crate) code: String,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct AskRequest {
    pub(crate) query: String,
    pub(crate) lang: Option<String>,
    #[serde(default, alias = "topK")]
    pub(crate) top_k: Option<usize>,
    #[serde(default, alias = "scopePaths")]
    pub(crate) scope_paths: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SetWatchRootRequest {
    pub(crate) path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ListProviderModelsRequest {
    pub(crate) provider: String,
    pub(crate) endpoint: String,
    pub(crate) api_key: Option<String>,
    pub(crate) models_root: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ProbeProviderRequest {
    pub(crate) provider: String,
    pub(crate) endpoint: String,
    pub(crate) api_key: Option<String>,
    pub(crate) models_root: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct PullModelRequest {
    pub(crate) model: String,
    pub(crate) provider: String,
    pub(crate) endpoint: String,
    pub(crate) api_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct OidcLoginRequest {
    pub(crate) id_token: Option<String>,
    pub(crate) access_token: Option<String>,
    pub(crate) subject: Option<String>,
    pub(crate) role: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct OidcLoginResponse {
    pub(crate) session_token: String,
    pub(crate) subject: String,
    pub(crate) role: Role,
    pub(crate) expires_at: i64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub(crate) struct AuditQuery {
    pub(crate) page: Option<usize>,
    pub(crate) page_size: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AuditListResponse {
    pub(crate) total: usize,
    pub(crate) page: usize,
    pub(crate) page_size: usize,
    pub(crate) items: Vec<AuditEventDto>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SetLocalModelsRootRequest {
    pub(crate) path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ScanLocalModelFilesRequest {
    pub(crate) root: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SettingsSearchCandidate {
    pub(crate) key: String,
    pub(crate) text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RankSettingsRequest {
    pub(crate) query: String,
    pub(crate) candidates: Vec<SettingsSearchCandidate>,
    pub(crate) lang: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RankSettingsResponse {
    pub(crate) keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct HealthResponse {
    pub(crate) ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub(crate) struct McpSettingsDto {
    pub(crate) enabled: bool,
    pub(crate) transports: Vec<String>,
    pub(crate) http_bind: String,
    pub(crate) http_port: u16,
    pub(crate) access_mode: String,
    pub(crate) audit_enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
pub(crate) struct McpStatusDto {
    pub(crate) enabled: bool,
    pub(crate) protocol_version: String,
    pub(crate) http_endpoint: String,
    pub(crate) stdio_command: String,
    pub(crate) tools_count: usize,
    pub(crate) resources_count: usize,
    pub(crate) prompts_count: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct ErrorResponse {
    pub(crate) error: String,
}

#[derive(Debug)]
pub(crate) struct ApiError {
    pub(crate) status: StatusCode,
    pub(crate) message: String,
}

impl ApiError {
    pub(crate) fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    pub(crate) fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }

    pub(crate) fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
        }
    }

    pub(crate) fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
        }
    }

    pub(crate) fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: message.into(),
        }
    }

    pub(crate) fn service_unavailable(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResponse {
                error: self.message,
            }),
        )
            .into_response()
    }
}
