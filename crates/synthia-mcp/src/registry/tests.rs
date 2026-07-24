//! Unit tests for the `registry` module family.
//!
//! Coverage map (6 tests):
//!
//! - `Registry<McpServerInfo>`: 4 tests
//!   ([`test_register_and_get`], [`test_unregister`],
//!   [`test_list_with_filter`], [`test_already_exists`]).
//! - `contains` / `len` / `is_empty`: 1 test
//!   ([`test_contains_and_len`]).
//! - `From<&McpServerConfig>`: 1 test
//!   ([`test_from_config`]).

use synthia_core::Registry;

use super::*;
use crate::types::McpServerConfig;
#[tokio::test]
async fn test_register_and_get() {
    let registry = McpRegistry::new();

    let info = McpServerInfo {
        id: "test-server".to_string(),
        name: "Test Server".to_string(),
        description: "A test MCP server".to_string(),
        command: "npx".to_string(),
        args: vec!["test-server".to_string()],
        enabled: true,
    };

    let result = registry.register(info.clone()).await;
    assert!(result.is_ok());

    let retrieved = registry.get("test-server").await.unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().id, "test-server");
}

#[tokio::test]
async fn test_unregister() {
    let registry = McpRegistry::new();

    let info = McpServerInfo {
        id: "test-server".to_string(),
        name: "Test Server".to_string(),
        description: "A test MCP server".to_string(),
        command: "npx".to_string(),
        args: vec![],
        enabled: true,
    };

    registry.register(info).await.unwrap();

    let result = registry.unregister("test-server").await;
    assert!(result.is_ok());

    let retrieved = registry.get("test-server").await.unwrap();
    assert!(retrieved.is_none());
}

#[tokio::test]
async fn test_list_with_filter() {
    let registry = McpRegistry::new();

    let server1 = McpServerInfo {
        id: "stdio-server".to_string(),
        name: "Stdio Server".to_string(),
        description: "Stdio transport".to_string(),
        command: "npx test-server".to_string(),
        args: vec![],
        enabled: true,
    };

    let server2 = McpServerInfo {
        id: "http-server".to_string(),
        name: "HTTP Server".to_string(),
        description: "HTTP transport".to_string(),
        command: "https://example.com/server".to_string(),
        args: vec![],
        enabled: false,
    };

    registry.register(server1).await.unwrap();
    registry.register(server2).await.unwrap();

    let all = registry.list(None).await.unwrap();
    assert_eq!(all.len(), 2);

    let filter_enabled = McpFilter {
        transport_type: None,
        enabled_only: true,
    };
    let enabled = registry.list(Some(filter_enabled)).await.unwrap();
    assert_eq!(enabled.len(), 1);

    let filter_transport = McpFilter {
        transport_type: Some("stdio".to_string()),
        enabled_only: false,
    };
    let by_transport = registry.list(Some(filter_transport)).await.unwrap();
    assert_eq!(by_transport.len(), 1);
}

#[tokio::test]
async fn test_already_exists() {
    let registry = McpRegistry::new();

    let info = McpServerInfo {
        id: "test-server".to_string(),
        name: "Test Server".to_string(),
        description: "A test MCP server".to_string(),
        command: "npx".to_string(),
        args: vec![],
        enabled: true,
    };

    registry.register(info.clone()).await.unwrap();
    let result = registry.register(info).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_contains_and_len() {
    let registry = McpRegistry::new();

    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);

    let info = McpServerInfo {
        id: "test-server".to_string(),
        name: "Test Server".to_string(),
        description: "A test MCP server".to_string(),
        command: "npx".to_string(),
        args: vec![],
        enabled: true,
    };

    registry.register(info).await.unwrap();

    assert!(!registry.is_empty());
    assert_eq!(registry.len(), 1);
    assert!(registry.contains("test-server"));
}

#[tokio::test]
async fn test_from_config() {
    let config = McpServerConfig {
        name: "test".to_string(),
        command: "npx test-server".to_string(),
        args: vec!["--flag".to_string()],
        env: std::collections::HashMap::new(),
    };

    let info = McpServerInfo::from(&config);
    assert_eq!(info.id, "test");
    assert_eq!(info.name, "test");
    assert!(info.description.contains("stdio"));
}
