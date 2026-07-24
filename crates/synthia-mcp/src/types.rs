use serde::{Deserialize, Serialize};
use thiserror::Error;

/// MCP protocol error codes (JSON-RPC extension range -32000 to -32099).
/// Based on JSON-RPC 2.0 and MCP specification conventions.
pub mod mcp_codes {
    /// Authentication required or credentials are invalid.
    pub const AUTH_ERROR: i64 = -32001;
    /// The provided access token is expired or has been revoked.
    pub const TOKEN_EXPIRED: i64 = -32002;
    /// Authorization failed - the client lacks required permissions.
    pub const AUTH_FORBIDDEN: i64 = -32003;
}

#[derive(Debug, Error)]
pub enum McpError {
    #[error("connection error: {0}")]
    Connection(#[from] std::io::Error),

    #[error("server not found: {0}")]
    ServerNotFound(String),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Authentication error with MCP error code.
    #[error("authentication error: {0}")]
    AuthError(String),

    /// Token expired or revoked.
    #[error("token expired: {0}")]
    TokenExpired(String),

    /// Authorization forbidden.
    #[error("authorization forbidden: {0}")]
    AuthForbidden(String),
}

impl McpError {
    /// Returns the MCP error code for this error, if applicable.
    pub fn mcp_code(&self) -> Option<i64> {
        match self {
            McpError::AuthError(_) => Some(mcp_codes::AUTH_ERROR),
            McpError::TokenExpired(_) => Some(mcp_codes::TOKEN_EXPIRED),
            McpError::AuthForbidden(_) => Some(mcp_codes::AUTH_FORBIDDEN),
            _ => None,
        }
    }

    /// Check if this error code represents an authentication error.
    pub fn is_auth_error_code(code: i64) -> bool {
        matches!(
            code,
            mcp_codes::AUTH_ERROR
                | mcp_codes::TOKEN_EXPIRED
                | mcp_codes::AUTH_FORBIDDEN
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct McpConnectionInfo {
    pub server_name: String,
    pub pid: Option<u32>,
    pub status: ConnectionStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStatus {
    Starting,
    Connected,
    Disconnected,
    Error,
}

#[derive(Debug, Clone)]
pub struct ToolSummary {
    pub name: String,
    pub description: String,
}

pub struct McpClient {
    pub server_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_summary() {
        let summary = ToolSummary {
            name: "search".to_string(),
            description: "Search the web".to_string(),
        };
        assert_eq!(summary.name, "search");
    }

    #[test]
    fn test_connection_status() {
        assert_ne!(ConnectionStatus::Connected, ConnectionStatus::Disconnected);
    }
}
