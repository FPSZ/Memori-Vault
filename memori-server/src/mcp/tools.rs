use serde_json::{Value as JsonValue, json};

use super::protocol::*;
use super::parse_params;
use crate::*;
use crate::mcp::tools_impl::*;

const TOOL_ASK: &str = "ask";
const TOOL_SEARCH: &str = "search";
const TOOL_GET_SOURCE: &str = "get_source";
const TOOL_OPEN_SOURCE: &str = "open_source";
const TOOL_GET_VAULT_STATS: &str = "get_vault_stats";
const TOOL_GET_INDEXING_STATUS: &str = "get_indexing_status";
const TOOL_TRIGGER_REINDEX: &str = "trigger_reindex";
const TOOL_PAUSE_INDEXING: &str = "pause_indexing";
const TOOL_RESUME_INDEXING: &str = "resume_indexing";
const TOOL_SET_INDEXING_MODE: &str = "set_indexing_mode";
const TOOL_SET_WATCH_ROOT: &str = "set_watch_root";
const TOOL_GET_MODEL_SETTINGS: &str = "get_model_settings";
const TOOL_SET_MODEL_SETTINGS: &str = "set_model_settings";
const TOOL_VALIDATE_MODEL_SETUP: &str = "validate_model_setup";
const TOOL_LIST_PROVIDER_MODELS: &str = "list_provider_models";
const TOOL_PROBE_MODEL_PROVIDER: &str = "probe_model_provider";
const TOOL_PULL_MODEL: &str = "pull_model";
const TOOL_GET_GRAPH_CONTEXT: &str = "get_graph_context";
const TOOL_GET_GRAPH_NEIGHBORS: &str = "get_graph_neighbors";
const TOOL_LIST_GRAPH_ENTITIES: &str = "list_graph_entities";
const TOOL_GET_APP_SETTINGS: &str = "get_app_settings";
const TOOL_GET_ENTERPRISE_POLICY: &str = "get_enterprise_policy";
const TOOL_GET_RUNTIME_BASELINE: &str = "get_runtime_baseline";
const TOOL_RANK_SETTINGS: &str = "rank_settings";
const TOOL_MEMORY_SEARCH: &str = "memory_search";
const TOOL_MEMORY_ADD: &str = "memory_add";
const TOOL_MEMORY_UPDATE: &str = "memory_update";
const TOOL_MEMORY_LIST_RECENT: &str = "memory_list_recent";
const TOOL_MEMORY_GET_SOURCE: &str = "memory_get_source";

pub fn list_tools() -> ListToolsResult {
    ListToolsResult {
        tools: vec![
            tool(
                TOOL_ASK,
                "Ask Memori-Vault and return answer, citations, evidence, and retrieval metrics.",
                schema(&["query"]),
            ),
            tool(
                TOOL_SEARCH,
                "Retrieve ranked source chunks without generating an answer.",
                schema(&["query"]),
            ),
            tool(
                TOOL_GET_SOURCE,
                "Read a source by file_path, chunk_id, or citation_index with a query.",
                schema(&[]),
            ),
            tool(
                TOOL_OPEN_SOURCE,
                "Open a local source file in the desktop environment.",
                schema(&["path"]),
            ),
            tool(
                TOOL_GET_VAULT_STATS,
                "Return vault document/chunk/graph statistics.",
                schema(&[]),
            ),
            tool(
                TOOL_GET_INDEXING_STATUS,
                "Return indexing phase, queue, and rebuild status.",
                schema(&[]),
            ),
            tool(
                TOOL_TRIGGER_REINDEX,
                "Trigger a full reindex task.",
                schema(&[]),
            ),
            tool(
                TOOL_PAUSE_INDEXING,
                "Pause background indexing.",
                schema(&[]),
            ),
            tool(
                TOOL_RESUME_INDEXING,
                "Resume background indexing.",
                schema(&[]),
            ),
            tool(
                TOOL_SET_INDEXING_MODE,
                "Set indexing mode/resource budget/schedule.",
                schema(&["indexing_mode", "resource_budget"]),
            ),
            tool(
                TOOL_SET_WATCH_ROOT,
                "Change the vault watch root and reload the engine.",
                schema(&["path"]),
            ),
            tool(
                TOOL_GET_MODEL_SETTINGS,
                "Return active local/remote model settings.",
                schema(&[]),
            ),
            tool(
                TOOL_SET_MODEL_SETTINGS,
                "Replace model settings and hot-reload runtime.",
                schema(&["active_provider", "local_profile", "remote_profile"]),
            ),
            tool(
                TOOL_VALIDATE_MODEL_SETUP,
                "Validate active model configuration.",
                schema(&[]),
            ),
            tool(
                TOOL_LIST_PROVIDER_MODELS,
                "List model candidates from provider/folder.",
                schema(&["provider", "endpoint"]),
            ),
            tool(
                TOOL_PROBE_MODEL_PROVIDER,
                "Probe a model provider endpoint.",
                schema(&["provider", "endpoint"]),
            ),
            tool(
                TOOL_PULL_MODEL,
                "Report that llama.cpp local runtime does not support model pulling.",
                schema(&["model", "provider", "endpoint"]),
            ),
            tool(
                TOOL_GET_GRAPH_CONTEXT,
                "Return graph context for a query or chunk ids.",
                schema(&[]),
            ),
            tool(
                TOOL_GET_GRAPH_NEIGHBORS,
                "Return 1-hop graph neighbors and source chunks for an entity id.",
                schema(&["entity_id"]),
            ),
            tool(
                TOOL_LIST_GRAPH_ENTITIES,
                "Search graph entities by id/name/label/description.",
                schema(&["query"]),
            ),
            tool(
                TOOL_GET_APP_SETTINGS,
                "Return app settings such as watch root and indexing config.",
                schema(&[]),
            ),
            tool(
                TOOL_GET_ENTERPRISE_POLICY,
                "Return enterprise egress/model policy.",
                schema(&[]),
            ),
            tool(
                TOOL_GET_RUNTIME_BASELINE,
                "Return runtime retrieval baseline diagnostics.",
                schema(&[]),
            ),
            tool(
                TOOL_RANK_SETTINGS,
                "Rank settings tabs for a natural-language settings query.",
                schema(&["query", "candidates"]),
            ),
            tool(
                TOOL_MEMORY_SEARCH,
                "Search STM/MTM/LTM memories without mixing them into document citations.",
                schema(&["query"]),
            ),
            tool(
                TOOL_MEMORY_ADD,
                "Add an audited long-term memory note/fact with explicit scope and source_ref.",
                schema(&["scope", "memory_type", "content", "source_ref"]),
            ),
            tool(
                TOOL_MEMORY_UPDATE,
                "Update or supersede an existing memory and write lifecycle audit log.",
                schema(&["memory_id"]),
            ),
            tool(
                TOOL_MEMORY_LIST_RECENT,
                "List recent memories for a scope.",
                schema(&[]),
            ),
            tool(
                TOOL_MEMORY_GET_SOURCE,
                "Read a memory and its lifecycle logs/source pointer.",
                schema(&["memory_id"]),
            ),
        ],
    }
}

pub async fn call_tool(
    state: ServerState,
    params: Option<JsonValue>,
) -> Result<JsonValue, JsonRpcError> {
    let call = parse_params::<CallToolParams>(params)?;
    let result = match call.name.as_str() {
        TOOL_ASK => ask(state, call.arguments).await,
        TOOL_SEARCH => search(state, call.arguments).await,
        TOOL_GET_SOURCE => get_source(state, call.arguments).await,
        TOOL_OPEN_SOURCE => open_source(call.arguments).await,
        TOOL_GET_VAULT_STATS => get_vault_stats(state).await,
        TOOL_GET_INDEXING_STATUS => get_indexing_status(state).await,
        TOOL_TRIGGER_REINDEX => trigger_reindex(state).await,
        TOOL_PAUSE_INDEXING => pause_indexing(state).await,
        TOOL_RESUME_INDEXING => resume_indexing(state).await,
        TOOL_SET_INDEXING_MODE => set_indexing_mode(state, call.arguments).await,
        TOOL_SET_WATCH_ROOT => set_watch_root(state, call.arguments).await,
        TOOL_GET_MODEL_SETTINGS => Ok(json!(resolve_model_settings(
            &load_app_settings().map_err(JsonRpcError::internal_error)?
        ))),
        TOOL_SET_MODEL_SETTINGS => set_model_settings(state, call.arguments).await,
        TOOL_VALIDATE_MODEL_SETUP => validate_model_setup().await,
        TOOL_LIST_PROVIDER_MODELS => list_provider_models(call.arguments).await,
        TOOL_PROBE_MODEL_PROVIDER => probe_model_provider(call.arguments).await,
        TOOL_PULL_MODEL => pull_model(call.arguments).await,
        TOOL_GET_GRAPH_CONTEXT => get_graph_context(state, call.arguments).await,
        TOOL_GET_GRAPH_NEIGHBORS => get_graph_neighbors(state, call.arguments).await,
        TOOL_LIST_GRAPH_ENTITIES => list_graph_entities(state, call.arguments).await,
        TOOL_GET_APP_SETTINGS => get_app_settings_value(),
        TOOL_GET_ENTERPRISE_POLICY => Ok(json!(resolve_enterprise_policy(
            &load_app_settings().map_err(JsonRpcError::internal_error)?
        ))),
        TOOL_GET_RUNTIME_BASELINE => get_runtime_baseline(state).await,
        TOOL_RANK_SETTINGS => rank_settings(state, call.arguments).await,
        TOOL_MEMORY_SEARCH => memory_search(state, call.arguments).await,
        TOOL_MEMORY_ADD => memory_add(state, call.arguments).await,
        TOOL_MEMORY_UPDATE => memory_update(state, call.arguments).await,
        TOOL_MEMORY_LIST_RECENT => memory_list_recent(state, call.arguments).await,
        TOOL_MEMORY_GET_SOURCE => memory_get_source(state, call.arguments).await,
        _ => Err(JsonRpcError::method_not_found(format!(
            "unknown tool: {}",
            call.name
        ))),
    };

    match result {
        Ok(value) => to_tool_result(value, false),
        Err(error) => to_tool_result(json!({ "error": error.message }), true),
    }
}
