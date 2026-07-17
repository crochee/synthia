//! Built-in tool providers for migrating static tools.

use std::sync::Arc;

use crate::tools::dynamic_provider::ToolProvider;

pub mod bash_tools_provider;
pub mod external_hook_tools_provider;
pub mod file_tools_provider;
pub mod guardian_tools_provider;
pub mod mcp_tools_provider;
pub mod monitor_tools_provider;
pub mod plugin_cli_tools_provider;
pub mod search_tools_provider;
pub mod tool_search_tools_provider;

/// Built-in provider set used by callers that don't pass an explicit set.
///
/// Composes the file, bash, search, and MCP ToolsProviders alongside the
/// five spec-only empty-shell providers (guardian, monitor,
/// external_hook_tool, plugin_cli, tool_search) introduced in R11.
pub fn default_providers() -> Vec<Arc<dyn ToolProvider>> {
    vec![
        Arc::new(file_tools_provider::FileToolsProvider),
        Arc::new(bash_tools_provider::BashToolsProvider),
        Arc::new(search_tools_provider::SearchToolsProvider),
        Arc::new(mcp_tools_provider::MCPToolsProvider),
        Arc::new(guardian_tools_provider::GuardianToolsProvider),
        Arc::new(monitor_tools_provider::MonitorToolsProvider),
        Arc::new(external_hook_tools_provider::ExternalHookToolsProvider),
        Arc::new(plugin_cli_tools_provider::PluginCliToolsProvider),
        Arc::new(tool_search_tools_provider::ToolSearchToolsProvider),
    ]
}
