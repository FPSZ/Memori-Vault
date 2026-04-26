use std::io::{BufRead, Write};

use super::handle_json_rpc_request;
use super::protocol::{JsonRpcRequest, JsonRpcResponse};
use crate::ServerState;

pub(crate) async fn run_stdio_server(state: ServerState) -> Result<(), String> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line.map_err(|err| format!("read MCP stdio failed: {err}"))?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<JsonRpcRequest>(&line) {
            Ok(request) => handle_json_rpc_request(state.clone(), request).await,
            Err(err) => Some(JsonRpcResponse::failure(
                None,
                super::protocol::JsonRpcError::invalid_request(format!("invalid JSON-RPC: {err}")),
            )),
        };
        if let Some(response) = response {
            let encoded = serde_json::to_string(&response)
                .map_err(|err| format!("serialize MCP stdio failed: {err}"))?;
            writeln!(stdout, "{encoded}")
                .map_err(|err| format!("write MCP stdio failed: {err}"))?;
            stdout
                .flush()
                .map_err(|err| format!("flush MCP stdio failed: {err}"))?;
        }
    }
    Ok(())
}
