use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MCPServerConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MCPConfig {
    pub servers: HashMap<String, MCPServerConfig>,
}

#[derive(Debug, Error)]
pub enum MCPError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Server not found: {0}")]
    ServerNotFound(String),
    #[error("Tool call failed: {0}")]
    ToolCallFailed(String),
    #[error("Timeout: {0}")]
    Timeout(String),
    #[error("Protocol error: {0}")]
    ProtocolError(String),
}

pub struct MCPClient {
    name: String,
    config: MCPServerConfig,
}

impl MCPClient {
    pub fn new(name: String, config: MCPServerConfig) -> Self {
        Self { name, config }
    }

    pub async fn connect(&self) -> Result<(), MCPError> {
        Ok(())
    }

    pub async fn disconnect(&self) {}

    pub async fn list_tools(&self) -> Result<Vec<McpTool>, MCPError> {
        Ok(vec![])
    }

    pub async fn call_tool(
        &self,
        _name: &str,
        _arguments: Value,
    ) -> Result<Value, MCPError> {
        Err(MCPError::ToolCallFailed("MCP client not connected".to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

pub struct MCPManager {
    clients: HashMap<String, MCPClient>,
    tools: HashMap<String, String>,
    config: MCPConfig,
}

impl MCPManager {
    pub fn new(config: MCPConfig) -> Self {
        Self {
            clients: HashMap::new(),
            tools: HashMap::new(),
            config,
        }
    }

    pub async fn connect_server(&mut self, name: &str) -> Result<(), MCPError> {
        let server_config = self.config.servers.get(name)
            .ok_or_else(|| MCPError::ServerNotFound(name.to_string()))?;

        let client = MCPClient::new(name.to_string(), server_config.clone());
        client.connect().await?;

        self.clients.insert(name.to_string(), client);

        Ok(())
    }

    pub async fn disconnect_server(&mut self, name: &str) -> Result<(), MCPError> {
        if let Some(mut client) = self.clients.remove(name) {
            client.disconnect().await;
            for tool_name in self.tools.keys().cloned().collect::<Vec<_>>() {
                if self.tools.get(&tool_name) == Some(&name.to_string()) {
                    self.tools.remove(&tool_name);
                }
            }
            Ok(())
        } else {
            Err(MCPError::ServerNotFound(name.to_string()))
        }
    }

    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: Value,
    ) -> Result<Value, MCPError> {
        let server_name = self.tools.get(tool_name)
            .ok_or_else(|| MCPError::ToolCallFailed(format!("Unknown tool: {}", tool_name)))?;

        let client = self.clients.get(server_name)
            .ok_or_else(|| MCPError::ServerNotFound(server_name.clone()))?;

        client.call_tool(tool_name, arguments).await
    }

    pub fn list_tools(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    pub fn has_tool(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }
}

pub async fn load_mcp_config(config_path: &PathBuf) -> Result<MCPConfig, MCPError> {
    if !config_path.exists() {
        return Ok(MCPConfig { servers: HashMap::new() });
    }

    let content = tokio::fs::read_to_string(config_path)
        .await
        .map_err(|e| MCPError::ProtocolError(e.to_string()))?;

    serde_json::from_str(&content).map_err(|e| MCPError::ProtocolError(e.to_string()))
}

pub fn default_mcp_config() -> MCPConfig {
    MCPConfig {
        servers: HashMap::new(),
    }
}
