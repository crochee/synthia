//! Integration tests for the MCP manager that also exercise
//! `McpRegistry` and `McpToolAdapter`.
//!
//! Lives in a separate `integration_tests` submodule (rather than
//! `tests`) so it stays `#[cfg(test)]` on the lib build but is
//! distinguished from the pure manager unit tests in
//! [`super::tests`].

use std::sync::Arc;

use synthia_tool::traits::Tool;

use super::types::McpManager;
use crate::{
    discovery::ToolDefinition,
    registry::McpRegistry,
    tool_adapter::McpToolAdapter,
    types::McpServerConfig,
};

#[tokio::test]
async fn test_mcp_tool_adapter_with_manager() {
    let manager = Arc::new(McpManager::new());
    let tool_definition = ToolDefinition {
        name: "test-tool".to_string(),
        description: "A test tool".to_string(),
        input_schema: serde_json::json!({}),
    };
    let adapter = McpToolAdapter::new("test-server", tool_definition, manager);

    assert_eq!(adapter.name(), "test-tool");
    assert_eq!(adapter.description(), "A test tool");
}

#[tokio::test]
async fn test_mcp_registry_with_manager() {
    let manager = Arc::new(McpManager::new());
    let registry = McpRegistry::with_manager(manager);

    let tools = registry.get_tool_metadata("test-server").await;
    assert!(tools.is_empty());
}

#[tokio::test]
async fn test_mcp_registry_discover_all_empty() {
    let manager = Arc::new(McpManager::new());
    let registry = McpRegistry::with_manager(manager);

    let result = registry.discover_all_tools().await;
    assert!(result.is_ok());
    let tools = result.unwrap();
    assert!(tools.is_empty());
}

#[tokio::test]
async fn test_mcp_tool_adapter_call_not_connected() {
    let manager = Arc::new(McpManager::new());
    let tool_definition = ToolDefinition {
        name: "test-tool".to_string(),
        description: "A test tool".to_string(),
        input_schema: serde_json::json!({}),
    };
    let adapter =
        Arc::new(McpToolAdapter::new("test-server", tool_definition, manager));

    let input = synthia_tool::types::ToolInput {
        name: "test-tool".to_string(),
        input: serde_json::json!({"query": "test"}),
        context: synthia_tool::types::ToolExecutionContext::new(
            "s1".to_string(),
            std::path::PathBuf::from("/tmp"),
        ),
    };

    let output = adapter.call(input).await;
    assert!(output.is_error.unwrap_or(false));
}

#[tokio::test]
async fn test_mcp_tool_adapter_into_mcp_tool() {
    let manager = Arc::new(McpManager::new());
    let tool_definition = ToolDefinition {
        name: "test-tool".to_string(),
        description: "A test tool".to_string(),
        input_schema: serde_json::json!({}),
    };
    let adapter =
        Arc::new(McpToolAdapter::new("test-server", tool_definition, manager));

    let mcp_tool = adapter.clone().into_mcp_tool();
    assert_eq!(mcp_tool.name(), "test-tool");
    assert_eq!(mcp_tool.server_name(), "test-server");
}

#[tokio::test]
async fn test_mcp_adapter_tool_trait_call_not_connected() {
    let manager = Arc::new(McpManager::new());
    let tool_definition = ToolDefinition {
        name: "test-tool".to_string(),
        description: "A test tool".to_string(),
        input_schema: serde_json::json!({}),
    };
    let adapter: Arc<dyn synthia_tool::traits::Tool> =
        Arc::new(McpToolAdapter::new("test-server", tool_definition, manager));

    let input = synthia_tool::types::ToolInput {
        name: "test-tool".to_string(),
        input: serde_json::json!({}),
        context: synthia_tool::types::ToolExecutionContext::new(
            "s1".to_string(),
            std::path::PathBuf::from("/tmp"),
        ),
    };

    let output = adapter.call(input).await;
    assert!(output.is_error.unwrap_or(false));
}

#[tokio::test]
async fn test_mcp_registry_with_manager_and_config() {
    let manager = Arc::new(McpManager::new());
    let registry = McpRegistry::with_manager(manager);

    let config = McpServerConfig {
        name: "test-server".to_string(),
        command: "echo".to_string(),
        args: vec![],
        env: std::collections::HashMap::new(),
    };

    registry.add_config(config).await;

    let configs = registry.list_configs().await;
    assert_eq!(configs.len(), 1);
}

#[tokio::test]
async fn test_mcp_registry_schema_cache() {
    let registry = McpRegistry::new();

    registry
        .cache_tool_schema(
            "server1",
            "tool1",
            serde_json::json!({"type": "object"}),
        )
        .await;

    let schema = registry.get_tool_schema("server1", "tool1").await.unwrap();
    assert!(!schema.is_null());
}

#[tokio::test]
async fn test_mcp_registry_clear_schema_cache() {
    let registry = McpRegistry::new();

    registry
        .cache_tool_schema(
            "server1",
            "tool1",
            serde_json::json!({"type": "object"}),
        )
        .await;
    registry.clear_schema_cache().await;

    let schema = registry.get_tool_schema("server1", "tool1").await.unwrap();
    assert!(schema.is_object() && schema.as_object().unwrap().is_empty());
}

#[tokio::test]
async fn test_mcp_registry_remove_config() {
    let registry = McpRegistry::new();

    let config = McpServerConfig {
        name: "test-server".to_string(),
        command: "echo".to_string(),
        args: vec![],
        env: std::collections::HashMap::new(),
    };

    registry.add_config(config).await;
    assert!(registry.remove_config("test-server").await);

    let configs = registry.list_configs().await;
    assert!(configs.is_empty());
}
