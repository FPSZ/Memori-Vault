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
    pub(crate) window_x: Option<i32>,
    pub(crate) window_y: Option<i32>,
    pub(crate) window_width: Option<u32>,
    pub(crate) window_height: Option<u32>,
    pub(crate) window_maximized: Option<bool>,
    // legacy fields for backwards compatibility
    pub(crate) provider: Option<String>,
    pub(crate) endpoint: Option<String>,
    pub(crate) api_key: Option<String>,
    pub(crate) chat_model: Option<String>,
    pub(crate) graph_model: Option<String>,
    pub(crate) embed_model: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct EnterprisePolicyDto {
    pub(crate) egress_mode: EgressMode,
    pub(crate) allowed_model_endpoints: Vec<String>,
    pub(crate) allowed_models: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ModelErrorItem {
    pub(crate) code: String,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ModelAvailabilityDto {
    pub(crate) configured: bool,
    pub(crate) reachable: bool,
    pub(crate) models: Vec<String>,
    pub(crate) missing_roles: Vec<String>,
    pub(crate) errors: Vec<ModelErrorItem>,
    pub(crate) checked_provider: Option<String>,
    pub(crate) status_code: Option<String>,
    pub(crate) status_message: Option<String>,
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
pub(crate) struct SettingsSearchCandidate {
    pub(crate) key: String,
    pub(crate) text: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SearchScopeItem {
    pub(crate) path: String,
    pub(crate) name: String,
    pub(crate) relative_path: String,
    pub(crate) is_dir: bool,
    pub(crate) depth: usize,
}
