use super::*;

const MCP_PROTOCOL_VERSION: &str = "2025-11-25";
const MCP_TOOL_COUNT: usize = 25;
const MCP_RESOURCE_COUNT: usize = 5;
const MCP_PROMPT_COUNT: usize = 5;

#[tauri::command]
pub(crate) async fn get_mcp_settings() -> Result<McpSettingsDto, String> {
    let settings = load_app_settings()?;
    Ok(resolve_mcp_settings(&settings))
}

#[tauri::command]
pub(crate) async fn set_mcp_settings(payload: McpSettingsDto) -> Result<McpSettingsDto, String> {
    let normalized = normalize_mcp_settings(payload);
    let mut settings = load_app_settings()?;
    settings.mcp_enabled = Some(normalized.enabled);
    settings.mcp_transports = Some(normalized.transports.clone());
    settings.mcp_http_bind = Some(normalized.http_bind.clone());
    settings.mcp_http_port = Some(normalized.http_port);
    settings.mcp_access_mode = Some(normalized.access_mode.clone());
    settings.mcp_audit_enabled = Some(normalized.audit_enabled);
    save_app_settings(&settings)?;
    Ok(normalized)
}

#[tauri::command]
pub(crate) async fn get_mcp_status() -> Result<McpStatusDto, String> {
    let settings = resolve_mcp_settings(&load_app_settings()?);
    Ok(build_mcp_status(&settings))
}

#[tauri::command]
pub(crate) async fn copy_mcp_client_config(client: String) -> Result<String, String> {
    let settings = resolve_mcp_settings(&load_app_settings()?);
    let status = build_mcp_status(&settings);
    let client = client.trim().to_ascii_lowercase();
    let config = if client == "http" {
        serde_json::json!({
            "mcpServers": {
                "memori-vault": {
                    "url": status.http_endpoint
                }
            }
        })
    } else {
        serde_json::json!({
            "mcpServers": {
                "memori-vault": {
                    "command": "memori-server",
                    "args": ["--mcp-stdio"]
                }
            }
        })
    };
    serde_json::to_string_pretty(&config).map_err(|err| err.to_string())
}

pub(crate) fn resolve_mcp_settings(settings: &AppSettings) -> McpSettingsDto {
    normalize_mcp_settings(McpSettingsDto {
        enabled: settings.mcp_enabled.unwrap_or(false),
        transports: settings
            .mcp_transports
            .clone()
            .unwrap_or_else(|| vec!["stdio".to_string(), "http".to_string()]),
        http_bind: settings
            .mcp_http_bind
            .clone()
            .unwrap_or_else(|| "127.0.0.1".to_string()),
        http_port: settings.mcp_http_port.unwrap_or(3757),
        access_mode: settings
            .mcp_access_mode
            .clone()
            .unwrap_or_else(|| "full_control".to_string()),
        audit_enabled: settings.mcp_audit_enabled.unwrap_or(true),
    })
}

fn normalize_mcp_settings(payload: McpSettingsDto) -> McpSettingsDto {
    let mut transports = payload
        .transports
        .into_iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| value == "stdio" || value == "http")
        .collect::<Vec<_>>();
    transports.sort();
    transports.dedup();
    if transports.is_empty() {
        transports = vec!["stdio".to_string(), "http".to_string()];
    }
    let http_bind = if payload.http_bind.trim().is_empty() {
        "127.0.0.1".to_string()
    } else {
        payload.http_bind.trim().to_string()
    };
    let http_port = if payload.http_port == 0 {
        3757
    } else {
        payload.http_port
    };
    let access_mode = match payload.access_mode.trim() {
        "read_only" => "read_only",
        _ => "full_control",
    }
    .to_string();
    McpSettingsDto {
        enabled: payload.enabled,
        transports,
        http_bind,
        http_port,
        access_mode,
        audit_enabled: payload.audit_enabled,
    }
}

fn build_mcp_status(settings: &McpSettingsDto) -> McpStatusDto {
    McpStatusDto {
        enabled: settings.enabled,
        protocol_version: MCP_PROTOCOL_VERSION.to_string(),
        http_endpoint: format!("http://{}:{}/mcp", settings.http_bind, settings.http_port),
        stdio_command: "memori-server --mcp-stdio".to_string(),
        tools_count: MCP_TOOL_COUNT,
        resources_count: MCP_RESOURCE_COUNT,
        prompts_count: MCP_PROMPT_COUNT,
    }
}
