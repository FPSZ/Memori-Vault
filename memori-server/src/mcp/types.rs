#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// Settings persisted for the MCP server
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpServerSettings {
    pub enabled: bool,
    pub transport: McpTransport,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpTransport {
    Stdio,
    Sse,
    #[default]
    Disabled,
}

/// Request DTO from the React UI
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpSettingsDto {
    pub enabled: bool,
    pub transport: McpTransportDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpTransportDto {
    Stdio,
    Sse,
}

/// Session state for an active MCP connection
#[derive(Debug, Clone)]
pub struct McpSession {
    pub initialized: bool,
    pub client_info: Option<String>,
}

impl McpSession {
    pub fn new() -> Self {
        Self {
            initialized: false,
            client_info: None,
        }
    }
}

/// Internal result for tool execution
#[derive(Debug, Clone)]
pub enum ToolExecutionResult {
    Ok(String),
    Err(String),
}
