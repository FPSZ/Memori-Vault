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
    pub(crate) local_chat_endpoint: Option<String>,
    pub(crate) local_graph_endpoint: Option<String>,
    pub(crate) local_embed_endpoint: Option<String>,
    pub(crate) local_models_root: Option<String>,
    pub(crate) local_llama_server_path: Option<String>,
    pub(crate) local_chat_model: Option<String>,
    pub(crate) local_graph_model: Option<String>,
    pub(crate) local_embed_model: Option<String>,
    pub(crate) local_chat_model_path: Option<String>,
    pub(crate) local_graph_model_path: Option<String>,
    pub(crate) local_embed_model_path: Option<String>,
    pub(crate) local_chat_context_length: Option<u32>,
    pub(crate) local_graph_context_length: Option<u32>,
    pub(crate) local_embed_context_length: Option<u32>,
    pub(crate) local_chat_concurrency: Option<u32>,
    pub(crate) local_graph_concurrency: Option<u32>,
    pub(crate) local_embed_concurrency: Option<u32>,
    pub(crate) remote_endpoint: Option<String>,
    pub(crate) remote_chat_endpoint: Option<String>,
    pub(crate) remote_graph_endpoint: Option<String>,
    pub(crate) remote_embed_endpoint: Option<String>,
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
    pub(crate) window_x: Option<i32>,
    pub(crate) window_y: Option<i32>,
    pub(crate) window_width: Option<u32>,
    pub(crate) window_height: Option<u32>,
    pub(crate) window_maximized: Option<bool>,
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
            memory_markdown_export_enabled: false,
            default_context_budget: settings
                .default_context_budget
                .unwrap_or_else(|| "16k".to_string()),
            complex_context_budget: settings
                .complex_context_budget
                .unwrap_or_else(|| "32k".to_string()),
            graph_ranking_enabled: false,
        }
    }
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
    pub(crate) chat_endpoint: String,
    pub(crate) graph_endpoint: String,
    pub(crate) embed_endpoint: String,
    pub(crate) models_root: Option<String>,
    #[serde(default)]
    pub(crate) llama_server_path: Option<String>,
    pub(crate) chat_model: String,
    pub(crate) graph_model: String,
    pub(crate) embed_model: String,
    #[serde(default)]
    pub(crate) chat_model_path: Option<String>,
    #[serde(default)]
    pub(crate) graph_model_path: Option<String>,
    #[serde(default)]
    pub(crate) embed_model_path: Option<String>,
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
    pub(crate) chat_endpoint: String,
    pub(crate) graph_endpoint: String,
    pub(crate) embed_endpoint: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LocalModelRuntimeStatusDto {
    pub(crate) role: String,
    pub(crate) endpoint: String,
    pub(crate) port: Option<u16>,
    pub(crate) pid: Option<u32>,
    pub(crate) state: String,
    pub(crate) message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LocalModelRuntimeStatusesDto {
    pub(crate) roles: Vec<LocalModelRuntimeStatusDto>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct McpSettingsDto {
    pub(crate) enabled: bool,
    pub(crate) transports: Vec<String>,
    pub(crate) http_bind: String,
    pub(crate) http_port: u16,
    pub(crate) access_mode: String,
    pub(crate) audit_enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct McpStatusDto {
    pub(crate) enabled: bool,
    pub(crate) protocol_version: String,
    pub(crate) http_endpoint: String,
    pub(crate) stdio_command: String,
    pub(crate) tools_count: usize,
    pub(crate) resources_count: usize,
    pub(crate) prompts_count: usize,
}
