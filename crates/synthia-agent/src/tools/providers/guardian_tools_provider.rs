//! Built-in tool provider for guardian-related tools.
//!
//! Spec-only placeholder: this provider is on the build path to
//! prove the `guardian` abstraction is addressable via the dynamic
//! provider framework. Concrete `ToolDefinition` entries will be
//! added when the underlying guardian tool surface stabilizes.

use async_trait::async_trait;

use crate::tools::dynamic_provider::{ToolDefinition, ToolProvider};

/// Provider for guardian-related tools. Currently exposes no
/// concrete entries — the runtime guardian surface is still
/// being migrated.
#[derive(Clone)]
pub struct GuardianToolsProvider;

impl GuardianToolsProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for GuardianToolsProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolProvider for GuardianToolsProvider {
    fn name(&self) -> &'static str {
        "guardian"
    }

    fn list_tools(&self) -> Vec<ToolDefinition> {
        vec![]
    }
}
