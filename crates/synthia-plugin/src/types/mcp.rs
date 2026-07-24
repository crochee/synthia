//! MCP types for the plugin MCP proxy system.
//!
//! Defines core types for MCP server configuration and transport modes.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Transport mode for MCP server communication
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    /// Standard I/O: spawn a local child process
    #[default]
    Stdio,
    /// Server-Sent Events over HTTP
    Sse,
    /// Plain HTTP/REST
    Http,
    /// WebSocket
    Ws,
}

impl Transport {
    /// Returns true if this is a network-based transport
    pub fn is_network(&self) -> bool {
        matches!(self, Transport::Sse | Transport::Http | Transport::Ws)
    }
}

impl fmt::Display for Transport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Transport::Stdio => write!(f, "stdio"),
            Transport::Sse => write!(f, "sse"),
            Transport::Http => write!(f, "http"),
            Transport::Ws => write!(f, "ws"),
        }
    }
}

/// Errors that can occur during MCP configuration validation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpConfigError {
    /// Stdio transport requires a command
    MissingCommand,
    /// Network transport requires a URL
    MissingUrl(Transport),
}

impl std::fmt::Display for McpConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McpConfigError::MissingCommand => {
                write!(f, "stdio transport requires a command")
            }
            McpConfigError::MissingUrl(transport) => {
                write!(f, "{transport} transport requires a URL")
            }
        }
    }
}

impl std::error::Error for McpConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_default() {
        assert_eq!(Transport::default(), Transport::Stdio);
    }

    #[test]
    fn test_transport_is_network() {
        assert!(!Transport::Stdio.is_network());
        assert!(Transport::Sse.is_network());
        assert!(Transport::Http.is_network());
        assert!(Transport::Ws.is_network());
    }

    #[test]
    fn test_transport_serde() {
        let json = serde_json::to_string(&Transport::Sse).unwrap();
        assert_eq!(json, "\"sse\"");

        let parsed: Transport = serde_json::from_str("\"ws\"").unwrap();
        assert_eq!(parsed, Transport::Ws);
    }

    #[test]
    fn test_config_error_display() {
        let err = McpConfigError::MissingCommand;
        assert_eq!(err.to_string(), "stdio transport requires a command");

        let err = McpConfigError::MissingUrl(Transport::Http);
        assert_eq!(err.to_string(), "http transport requires a URL");
    }
}
