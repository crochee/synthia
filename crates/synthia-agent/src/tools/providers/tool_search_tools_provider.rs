//! Built-in tool provider for tool-search tools.
//!
//! Spec-only placeholder: this provider is on the build path to
//! prove the `tool_search` abstraction is addressable via the
//! dynamic provider framework. Concrete `ToolDefinition` entries
//! will be added when the underlying tool-search surface
//! stabilizes.

use async_trait::async_trait;

use crate::tools::dynamic_provider::{ToolDefinition, ToolProvider};

/// Provider for tool-search tools. Currently exposes no
/// concrete entries — the runtime tool-search surface is still
/// being migrated.
#[derive(Clone)]
pub struct ToolSearchToolsProvider;

impl ToolSearchToolsProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ToolSearchToolsProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolProvider for ToolSearchToolsProvider {
    fn name(&self) -> &'static str {
        "tool_search"
    }

    fn list_tools(&self) -> Vec<ToolDefinition> {
        vec![]
    }
}
