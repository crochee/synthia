//! MCP connection trait and transport configuration.

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::tool::descriptor::ToolDescriptor;

/// Errors that can occur during MCP operations.
#[derive(Debug, Error, Serialize, Deserialize)]
pub enum McpError {
    /// Failed to establish a connection to the MCP server.
    #[error("connection failed: {0}")]
    ConnectionFailed(String),
    /// The requested tool was not found on the MCP server.
    #[error("tool not found: {0}")]
    ToolNotFound(String),
    /// Tool execution failed.
    #[error("execution failed: {0}")]
    ExecutionFailed(String),
    /// Operation timed out.
    #[error("timeout")]
    Timeout,
}

/// Trait for an MCP server connection (object-safe).
#[async_trait]
pub trait McpConnection: Send + Sync {
    /// Returns the MCP server name.
    fn server_name(&self) -> &str;

    /// Connect to the MCP server.
    async fn connect(&self) -> Result<(), McpError>;

    /// Close the connection to the MCP server.
    async fn close(&self) -> Result<(), McpError>;

    /// List all tools available on the MCP server.
    async fn list_tools(&self) -> Result<Vec<ToolDescriptor>, McpError>;

    /// Call a tool on the MCP server by name with the given input.
    async fn call_tool(
        &self,
        name: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, McpError>;
}

/// Transport configuration for connecting to an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum McpTransportConfig {
    /// Spawn a child process and communicate over stdin/stdout.
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
    },
    /// Connect via streamable HTTP (SSE + POST).
    StreamableHttp {
        url: String,
        #[serde(default)]
        headers: HashMap<String, String>,
    },
    /// Connect via WebSocket.
    WebSocket { url: String },
}
