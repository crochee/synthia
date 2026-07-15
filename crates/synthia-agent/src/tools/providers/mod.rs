//! Built-in tool providers for migrating static tools.

use std::sync::Arc;

use crate::tools::dynamic_provider::ToolProvider;

pub mod bash_tools_provider;
pub mod file_tools_provider;
pub mod mcp_tools_provider;
pub mod search_tools_provider;

/// Built-in provider set used by callers that don't pass an explicit set.
/// Composes the file, bash, search, and MCP ToolsProviders.
pub fn default_providers() -> Vec<Arc<dyn ToolProvider>> {
    vec![
        Arc::new(file_tools_provider::FileToolsProvider),
        Arc::new(bash_tools_provider::BashToolsProvider),
        Arc::new(search_tools_provider::SearchToolsProvider),
        Arc::new(mcp_tools_provider::MCPToolsProvider),
    ]
}
