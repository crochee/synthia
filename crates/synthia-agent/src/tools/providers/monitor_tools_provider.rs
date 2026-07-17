//! Built-in tool provider for monitor-related tools.
//!
//! Spec-only placeholder: this provider is on the build path to
//! prove the `monitor` abstraction is addressable via the dynamic
//! provider framework. Concrete `ToolDefinition` entries will be
//! added when the underlying monitor tool surface stabilizes.

use async_trait::async_trait;

use crate::tools::dynamic_provider::{ToolDefinition, ToolProvider};

/// Provider for monitor-related tools. Currently exposes no
/// concrete entries — the runtime monitor surface is still
/// being migrated.
#[derive(Clone)]
pub struct MonitorToolsProvider;

impl MonitorToolsProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MonitorToolsProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolProvider for MonitorToolsProvider {
    fn name(&self) -> &'static str {
        "monitor"
    }

    fn list_tools(&self) -> Vec<ToolDefinition> {
        vec![]
    }
}
