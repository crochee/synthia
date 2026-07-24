//! MCP types for API requests and responses

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::config::{
    DEFAULT_MCP_ENABLED,
    DEFAULT_MCP_SERVER_TYPE,
    DEFAULT_MCP_TIMEOUT,
};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpServerConfig {
    pub name: String,
    #[serde(rename = "type", default = "default_server_type")]
    pub server_type: String,
    pub description: Option<String>,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_server_type() -> String {
    DEFAULT_MCP_SERVER_TYPE.to_string()
}

fn default_timeout() -> u64 {
    DEFAULT_MCP_TIMEOUT
}

fn default_enabled() -> bool {
    DEFAULT_MCP_ENABLED
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerStatus {
    pub name: String,
    pub status: String,
    pub description: Option<String>,
    pub tools: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct McpServerRequest {
    pub name: String,
    pub command: String,
    #[serde(default = "default_server_type")]
    pub server_type: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

impl From<McpServerRequest> for McpServerConfig {
    fn from(req: McpServerRequest) -> Self {
        Self {
            name: req.name,
            server_type: req.server_type,
            description: req.description,
            command: req.command,
            args: req.args,
            env: req.env,
            timeout: req.timeout,
            enabled: req.enabled,
        }
    }
}
