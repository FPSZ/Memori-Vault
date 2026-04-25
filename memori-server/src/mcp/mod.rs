use crate::*;

pub mod prompts;
pub mod protocol;
pub mod resources;
pub mod tools;
pub mod transport_http;
pub mod transport_stdio;
pub mod types;

use protocol::*;

pub(crate) async fn handle_json_rpc_request(
    state: ServerState,
    request: JsonRpcRequest,
) -> Option<JsonRpcResponse> {
    if request.jsonrpc != "2.0" {
        return Some(JsonRpcResponse::failure(
            request.id,
            JsonRpcError::invalid_request("jsonrpc must be 2.0"),
        ));
    }

    let is_notification = request.id.is_none();
    match request.method.as_str() {
        "initialize" => Some(handle_initialize(request.id, request.params)),
        "notifications/initialized" => None,
        "ping" => {
            if is_notification {
                None
            } else {
                Some(JsonRpcResponse::success_empty(request.id))
            }
        }
        "tools/list" => json_response(request.id, tools::list_tools()),
        "tools/call" => Some(to_response(
            request.id,
            tools::call_tool(state, request.params).await,
        )),
        "resources/list" => json_response(request.id, resources::list_resources()),
        "resources/templates/list" => json_response(request.id, resources::list_resource_templates()),
        "resources/read" => Some(to_response(
            request.id,
            resources::read_resource(state, request.params).await,
        )),
        "prompts/list" => json_response(request.id, prompts::list_prompts()),
        "prompts/get" => Some(to_response(request.id, prompts::get_prompt(request.params))),
        _ => {
            if is_notification {
                None
            } else {
                Some(JsonRpcResponse::failure(
                    request.id,
                    JsonRpcError::method_not_found(format!(
                        "unsupported MCP method: {}",
                        request.method
                    )),
                ))
            }
        }
    }
}

fn handle_initialize(id: Option<serde_json::Value>, params: Option<serde_json::Value>) -> JsonRpcResponse {
    let requested_version = params
        .as_ref()
        .and_then(|value| serde_json::from_value::<InitializeParams>(value.clone()).ok())
        .map(|params| params.protocol_version)
        .unwrap_or_else(|| MCP_PROTOCOL_VERSION.to_string());
    let protocol_version = if requested_version == MCP_COMPAT_PROTOCOL_VERSION {
        MCP_COMPAT_PROTOCOL_VERSION
    } else {
        MCP_PROTOCOL_VERSION
    };

    let result = InitializeResult {
        protocol_version: protocol_version.to_string(),
        capabilities: ServerCapabilities {
            tools: Some(ToolsCapability { list_changed: false }),
            resources: Some(ResourcesCapability {
                subscribe: false,
                list_changed: false,
            }),
            prompts: Some(PromptsCapability { list_changed: false }),
        },
        server_info: Implementation {
            name: "memori-vault".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
    };
    JsonRpcResponse::success(id, serde_json::to_value(result).unwrap_or_else(|_| serde_json::json!({})))
}

fn json_response<T: serde::Serialize>(id: Option<serde_json::Value>, result: T) -> Option<JsonRpcResponse> {
    Some(to_response(id, serde_json::to_value(result).map_err(|err| {
        JsonRpcError::internal_error(format!("serialize MCP result failed: {err}"))
    })))
}

fn to_response(
    id: Option<serde_json::Value>,
    result: Result<serde_json::Value, JsonRpcError>,
) -> JsonRpcResponse {
    match result {
        Ok(value) => JsonRpcResponse::success(id, value),
        Err(error) => JsonRpcResponse::failure(id, error),
    }
}

pub(crate) async fn engine_from_state(state: &ServerState) -> Result<MemoriEngine, JsonRpcError> {
    let init_error = state.init_error.lock().await.clone();
    let engine_guard = state.engine.lock().await;
    engine_guard.as_ref().cloned().ok_or_else(|| {
        JsonRpcError::internal_error(match init_error {
            Some(message) => format!("engine initialization failed: {message}"),
            None => "engine is still initializing".to_string(),
        })
    })
}

pub(crate) fn parse_params<T: serde::de::DeserializeOwned>(
    params: Option<serde_json::Value>,
) -> Result<T, JsonRpcError> {
    serde_json::from_value(params.unwrap_or_else(|| serde_json::json!({})))
        .map_err(|err| JsonRpcError::invalid_params(err.to_string()))
}

pub(crate) fn normalize_mcp_top_k(top_k: Option<usize>, default_value: usize) -> usize {
    top_k
        .filter(|value| (1..=50).contains(value))
        .unwrap_or(default_value)
}
