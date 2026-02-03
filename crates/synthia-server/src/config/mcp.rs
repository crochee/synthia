//! MCP server configuration types

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub const DEFAULT_MCP_TIMEOUT: u64 = 300;
pub const DEFAULT_MCP_ENABLED: bool = true;
pub const DEFAULT_MCP_SERVER_TYPE: &str = "stdio";

fn default_mcp_timeout() -> u64 {
    DEFAULT_MCP_TIMEOUT
}

fn default_mcp_enabled() -> bool {
    DEFAULT_MCP_ENABLED
}

fn default_mcp_server_type() -> String {
    DEFAULT_MCP_SERVER_TYPE.to_string()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpConfig {
    #[serde(rename = "type", default = "default_mcp_server_type")]
    pub mcp_type: String,
    #[serde(default)]
    pub description: Option<String>,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default = "default_mcp_timeout")]
    pub timeout: u64,
    #[serde(default = "default_mcp_enabled")]
    pub enabled: bool,
}

impl From<McpConfig> for crate::mcp::McpServerConfig {
    fn from(config: McpConfig) -> Self {
        Self {
            name: String::new(),
            server_type: config.mcp_type,
            description: config.description,
            command: config.command,
            args: config.args,
            env: config.env,
            timeout: config.timeout,
            enabled: config.enabled,
        }
    }
}
