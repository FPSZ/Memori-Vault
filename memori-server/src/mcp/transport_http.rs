use axum::Json;
use axum::extract::State;
use serde_json::json;

use super::handle_json_rpc_request;
use super::protocol::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};
use crate::ServerState;

pub(crate) async fn mcp_http_handler(
    State(state): State<ServerState>,
    Json(request): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    // 检查 MCP 是否启用
    let mcp_enabled = match crate::settings_io::load_app_settings() {
        Ok(settings) => settings.mcp_enabled.unwrap_or(false),
        Err(_) => false,
    };

    if !mcp_enabled {
        return Json(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: None,
            error: Some(JsonRpcError {
                code: -32001,
                message: "MCP is disabled. Enable it in Settings > MCP.".to_string(),
                data: None,
            }),
        });
    }

    let response = handle_json_rpc_request(state, request)
        .await
        .unwrap_or_else(|| JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: None,
            result: Some(json!({})),
            error: None,
        });
    Json(response)
}
