//! Unit tests for the `mcp_proxy` module family.
//!
//! Coverage map (10 tests):
//!
//! - Creation + empty state: 1 test
//!   ([`test_mcp_proxy_creation`], [`test_running_servers_empty`]).
//! - Config validation: 3 tests
//!   ([`test_stdio_server_config_validation`],
//!   [`test_network_server_config_validation`],
//!   [`test_mcp_server_config_fields`]).
//! - Transport discrimination: 1 test
//!   ([`test_transport_variants`]).
//! - Stop unknown server: 2 tests
//!   ([`test_start_unknown_server`],
//!   [`test_stop_nonexistent_server`]).
//! - `start_server` validation path: 1 test
//!   ([`test_start_server_validation`]).
//! - `is_running` false for unknown: 1 test
//!   ([`test_is_running_false_for_unknown`]).

use std::collections::HashMap;

use super::*;
use crate::{Transport, registry::McpServerConfig};

#[test]
fn test_mcp_proxy_creation() {
    let proxy = McpProxy::new(Vec::new());
    assert!(futures::executor::block_on(proxy.running_servers()).is_empty());
}

#[test]
fn test_stdio_server_config_validation() {
    let config = McpServerConfig {
        name: "test-server".to_string(),
        transport: Some(Transport::Stdio),
        command: Some("echo".to_string()),
        args: vec!["hello".to_string()],
        env: HashMap::new(),
        url: None,
    };

    assert!(config.validate().is_ok());
}

#[test]
fn test_network_server_config_validation() {
    let config = McpServerConfig {
        name: "remote-server".to_string(),
        transport: Some(Transport::Http),
        command: None,
        args: vec![],
        env: HashMap::new(),
        url: Some("https://api.example.com/mcp".to_string()),
    };

    assert!(config.validate().is_ok());
}

#[test]
fn test_start_unknown_server() {
    let proxy = McpProxy::new(Vec::new());
    let result = futures::executor::block_on(proxy.stop_server("nonexistent"));
    assert!(result.is_err());
}

#[test]
fn test_mcp_server_config_fields() {
    let mut env = HashMap::new();
    env.insert("NODE_ENV".to_string(), "production".to_string());

    let config = McpServerConfig {
        name: "my-server".to_string(),
        transport: Some(Transport::Stdio),
        command: Some("node".to_string()),
        args: vec!["server.js".to_string(), "--flag".to_string()],
        env,
        url: None,
    };

    assert_eq!(config.name, "my-server");
    assert_eq!(config.transport(), Transport::Stdio);
    assert_eq!(config.command, Some("node".to_string()));
    assert_eq!(config.args, vec!["server.js", "--flag"]);
    assert_eq!(config.env.get("NODE_ENV"), Some(&"production".to_string()));
}

#[test]
fn test_transport_variants() {
    assert!(!Transport::Stdio.is_network());
    assert!(Transport::Sse.is_network());
    assert!(Transport::Http.is_network());
    assert!(Transport::Ws.is_network());
}

#[tokio::test]
async fn test_start_server_validation() {
    let proxy = McpProxy::new(Vec::new());

    // Config without command or url should fail
    let config = McpServerConfig {
        name: "test".to_string(),
        transport: None,
        command: None,
        args: vec![],
        env: HashMap::new(),
        url: None,
    };
    let result = proxy.start_server("test", &config).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_stop_nonexistent_server() {
    let proxy = McpProxy::new(Vec::new());
    let result = proxy.stop_server("nonexistent").await;
    assert!(matches!(result, Err(McpProxyError::ServerNotFound(_))));
}

#[tokio::test]
async fn test_running_servers_empty() {
    let proxy = McpProxy::new(Vec::new());
    let servers = proxy.running_servers().await;
    assert!(servers.is_empty());
}

#[tokio::test]
async fn test_is_running_false_for_unknown() {
    let proxy = McpProxy::new(Vec::new());
    assert!(!proxy.is_running("unknown").await);
}
