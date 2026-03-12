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
    pub(crate) remote_endpoint: Option<String>,
    pub(crate) remote_api_key: Option<String>,
    pub(crate) remote_chat_model: Option<String>,
    pub(crate) remote_graph_model: Option<String>,
    pub(crate) remote_embed_model: Option<String>,
    pub(crate) enterprise_egress_mode: Option<String>,
    pub(crate) enterprise_allowed_model_endpoints: Option<Vec<String>>,
    pub(crate) enterprise_allowed_models: Option<Vec<String>>,
    pub(crate) oidc_issuer: Option<String>,
    pub(crate) oidc_client_id: Option<String>,
    pub(crate) oidc_redirect_uri: Option<String>,
    pub(crate) oidc_roles_claim: Option<String>,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RemoteModelProfileDto {
    pub(crate) endpoint: String,
    pub(crate) api_key: Option<String>,
    pub(crate) chat_model: String,
    pub(crate) graph_model: String,
    pub(crate) embed_model: String,
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
