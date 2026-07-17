//! Built-in tool provider for external hook tools.
//!
//! Spec-only placeholder: this provider is on the build path to
//! prove the `external_hook_tool` abstraction is addressable via
//! the dynamic provider framework. Concrete `ToolDefinition`
//! entries will be added when the underlying hook tool surface
//! stabilizes.

use async_trait::async_trait;

use crate::tools::dynamic_provider::{ToolDefinition, ToolProvider};

/// Provider for external hook tools. Currently exposes no
/// concrete entries — the runtime hook surface is still being
/// migrated.
#[derive(Clone)]
pub struct ExternalHookToolsProvider;

impl ExternalHookToolsProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ExternalHookToolsProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolProvider for ExternalHookToolsProvider {
    fn name(&self) -> &'static str {
        "external_hook_tool"
    }

    fn list_tools(&self) -> Vec<ToolDefinition> {
        vec![]
    }
}
