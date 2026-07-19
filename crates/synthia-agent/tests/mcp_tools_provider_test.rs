#![allow(deprecated)]
//! Integration test for `MCPToolsProvider`.
//!
//! Asserts that the provider publishes MCP-shaped tool definitions
//! that mirror the static `synthia-mcp` tool surface. The provider
//! is a placeholder for the real MCP tool list (which is populated
//! dynamically from `McpManager::discover`), but the names must
//! match what real MCP clients will produce so downstream
//! dispatch logic can locate them by string.

use synthia_agent::tools::{
    dynamic_provider::ToolProvider,
    providers::mcp_tools_provider::MCPToolsProvider,
};

#[test]
fn mcp_tools_provider_lists_at_least_one_tool() {
    let provider = MCPToolsProvider::new();
    let tools = provider.list_tools();

    assert!(
        !tools.is_empty(),
        "MCPToolsProvider must publish at least one tool, got 0 (provider name = {})",
        provider.name(),
    );
}

#[test]
fn mcp_tools_provider_name_is_mcp_tools() {
    let provider = MCPToolsProvider::new();
    assert_eq!(provider.name(), "mcp_tools");
}

#[test]
fn mcp_tools_provider_exposes_known_names() {
    let provider = MCPToolsProvider::new();
    let tools = provider.list_tools();
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();

    assert!(
        names.contains(&"mcp_echo"),
        "expected provider to expose the 'mcp_echo' tool, got {names:?}",
    );
}
