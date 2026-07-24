//! Unit tests for the MCP manager. Integration tests that exercise
//! `McpRegistry` / `McpToolAdapter` are in [`super::integration_tests`].

use std::time::Duration;

use super::types::McpManager;
use crate::server::IdleTimeoutConfig;

#[tokio::test]
async fn test_mcp_manager_start_nonexistent() {
    let manager = McpManager::new();
    let result = manager.start("nonexistent").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_mcp_manager_stop_all() {
    let manager = McpManager::new();
    manager.stop_all().await.unwrap();
    assert!(manager.connections.read().await.is_empty());
}

#[tokio::test]
async fn test_mcp_manager_idle_tracking() {
    let manager = McpManager::new();
    manager.record_activity("test-server").await;
    assert!(!manager.is_idle("test-server").await);
    // Server not started shouldn't be considered idle
    assert!(!manager.is_idle("unknown-server").await);
}

#[tokio::test]
async fn test_mcp_manager_discovery_cache() {
    let manager = McpManager::new();
    // No discovery available before connection
    assert!(manager.get_discovery("unknown").await.is_none());
}

#[tokio::test]
async fn test_mcp_manager_with_custom_config() {
    let config = IdleTimeoutConfig {
        timeout: Duration::from_secs(10),
        check_interval: Duration::from_secs(5),
    };
    let manager = McpManager::with_idle_config(config);
    assert_eq!(manager.idle_config.timeout, Duration::from_secs(10));
}

#[tokio::test]
async fn test_mcp_manager_restart() {
    let manager = McpManager::new();
    let result = manager.restart("nonexistent").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_mcp_manager_discover_tools_single_server() {
    let manager = McpManager::new();
    let result = manager.discover_tools_for_server("unknown").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_mcp_manager_is_connected() {
    let manager = McpManager::new();
    assert!(!manager.is_connected("unknown").await);
}

#[tokio::test]
async fn test_mcp_manager_get_idle_servers_empty() {
    let manager = McpManager::new();
    let idle = manager.get_idle_servers().await;
    assert!(idle.is_empty());
}

#[tokio::test]
async fn test_mcp_manager_recycle_idle_empty() {
    let manager = McpManager::new();
    let recycled = manager.recycle_idle_servers().await.unwrap();
    assert!(recycled.is_empty());
}

#[tokio::test]
async fn test_mcp_manager_status_unknown() {
    let manager = McpManager::new();
    assert!(manager.get_status("unknown").await.is_none());
}
