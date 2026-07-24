//! Unit tests for the `connection` module family.
//!
//! Coverage map (7 tests):
//!
//! - `ConnectionState`: 2 tests
//!   ([`test_connection_state_display`],
//!   [`test_connection_state_equality`]).
//! - `McpConnection`: 5 tests
//!   ([`test_mcp_connection_new`],
//!   [`test_mcp_connection_disconnect`],
//!   [`test_mcp_connection_update_last_used`],
//!   [`test_mcp_connection_get_tools`],
//!   [`test_mcp_connection_tools_mut`],
//!   [`test_mcp_connection_debug`]).

use super::*;
use crate::{McpServerConfig, ToolDefinition};

#[test]
fn test_connection_state_display() {
    assert_eq!(ConnectionState::Discovered.to_string(), "discovered");
    assert_eq!(ConnectionState::Connecting.to_string(), "connecting");
    assert_eq!(ConnectionState::Connected.to_string(), "connected");
    assert_eq!(ConnectionState::Idle.to_string(), "idle");
    assert_eq!(ConnectionState::Error.to_string(), "error");
}

#[test]
fn test_connection_state_equality() {
    assert_eq!(ConnectionState::Connected, ConnectionState::Connected);
    assert_ne!(ConnectionState::Connected, ConnectionState::Discovered);
}

#[tokio::test]
async fn test_mcp_connection_new() {
    let config = McpServerConfig {
        name: "test-server".to_string(),
        command: "echo".to_string(),
        args: vec![],
        env: std::collections::HashMap::new(),
    };
    let tools = vec![ToolDefinition {
        name: "test-tool".to_string(),
        description: "A test tool".to_string(),
        input_schema: serde_json::json!({}),
    }];

    let conn = McpConnection::new("test-server".to_string(), config, tools);

    assert_eq!(conn.server_id, "test-server");
    assert_eq!(conn.state, ConnectionState::Discovered);
    assert!(!conn.is_connected());
    assert!(conn.connected_at.is_none());
}

#[tokio::test]
async fn test_mcp_connection_disconnect() {
    let config = McpServerConfig {
        name: "test-server".to_string(),
        command: "echo".to_string(),
        args: vec![],
        env: std::collections::HashMap::new(),
    };
    let tools = vec![];

    let mut conn = McpConnection::new("test-server".to_string(), config, tools);
    conn.disconnect().await;

    assert_eq!(conn.state, ConnectionState::Idle);
    assert!(conn.connected_at.is_none());
}

#[tokio::test]
async fn test_mcp_connection_update_last_used() {
    let config = McpServerConfig {
        name: "test-server".to_string(),
        command: "echo".to_string(),
        args: vec![],
        env: std::collections::HashMap::new(),
    };
    let tools = vec![];

    let conn = McpConnection::new("test-server".to_string(), config, tools);
    conn.update_last_used();

    let duration = conn.last_used_duration();
    assert!(duration.as_secs() < 2);
}

#[tokio::test]
async fn test_mcp_connection_get_tools() {
    let config = McpServerConfig {
        name: "test-server".to_string(),
        command: "echo".to_string(),
        args: vec![],
        env: std::collections::HashMap::new(),
    };
    let tools = vec![
        ToolDefinition {
            name: "tool1".to_string(),
            description: "First tool".to_string(),
            input_schema: serde_json::json!({}),
        },
        ToolDefinition {
            name: "tool2".to_string(),
            description: "Second tool".to_string(),
            input_schema: serde_json::json!({}),
        },
    ];

    let conn = McpConnection::new("test-server".to_string(), config, tools);

    assert_eq!(conn.get_tools().len(), 2);
    assert_eq!(conn.get_tools()[0].name, "tool1");
    assert_eq!(conn.get_tools()[1].name, "tool2");
}

#[tokio::test]
async fn test_mcp_connection_tools_mut() {
    let config = McpServerConfig {
        name: "test-server".to_string(),
        command: "echo".to_string(),
        args: vec![],
        env: std::collections::HashMap::new(),
    };
    let tools = vec![];

    let mut conn = McpConnection::new("test-server".to_string(), config, tools);
    conn.tools_mut().push(ToolDefinition {
        name: "new-tool".to_string(),
        description: "A new tool".to_string(),
        input_schema: serde_json::json!({}),
    });

    assert_eq!(conn.get_tools().len(), 1);
    assert_eq!(conn.get_tools()[0].name, "new-tool");
}

#[test]
fn test_mcp_connection_debug() {
    let config = McpServerConfig {
        name: "test-server".to_string(),
        command: "echo".to_string(),
        args: vec![],
        env: std::collections::HashMap::new(),
    };
    let tools = vec![];

    let conn = McpConnection::new("test-server".to_string(), config, tools);
    let debug_str = format!("{:?}", conn);

    assert!(debug_str.contains("test-server"));
    assert!(debug_str.contains("Discovered"));
}
