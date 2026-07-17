//! Built-in tool provider for plugin CLI tools.
//!
//! Spec-only placeholder: this provider is on the build path to
//! prove the `plugin_cli` abstraction is addressable via the
//! dynamic provider framework. Concrete `ToolDefinition` entries
//! will be added when the underlying plugin CLI surface
//! stabilizes.

use async_trait::async_trait;

use crate::tools::dynamic_provider::{ToolDefinition, ToolProvider};

/// Provider for plugin CLI tools. Currently exposes no concrete
/// entries — the runtime plugin CLI surface is still being
/// migrated.
#[derive(Clone)]
pub struct PluginCliToolsProvider;

impl PluginCliToolsProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PluginCliToolsProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolProvider for PluginCliToolsProvider {
    fn name(&self) -> &'static str {
        "plugin_cli"
    }

    fn list_tools(&self) -> Vec<ToolDefinition> {
        vec![]
    }
}
