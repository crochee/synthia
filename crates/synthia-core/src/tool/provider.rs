//! ToolProvider trait — the registration contract for tools.

use std::sync::Arc;

use async_trait::async_trait;

use crate::tool::{
    descriptor::ToolDescriptor,
    types::{ToolError, ToolInput, ToolOutput},
};

/// Source of tools. Multiple providers can be registered.
#[async_trait]
pub trait ToolProvider: Send + Sync + 'static {
    /// Stable provider id.
    fn id(&self) -> &str;

    /// List all tools this provider exposes.
    async fn list_tools(&self) -> Vec<ToolDescriptor>;

    /// Get a tool by name.
    async fn get_tool(
        &self,
        name: &str,
    ) -> Option<Arc<dyn crate::tool::descriptor::Tool>>;

    /// Event callback for tool lifecycle events.
    async fn on_tool_event(&self, _event: &ToolEvent) {}

    /// Pre-execution hook. Default: no-op.
    async fn before_execute(&self, _call: &ToolCall) -> Result<(), ToolError> {
        Ok(())
    }

    /// Post-execution hook. Default: no-op.
    async fn after_execute(&self, _call: &ToolCall, _result: &ToolOutput) {}
}

/// Tool lifecycle event.
#[derive(Debug, Clone)]
pub enum ToolEvent {
    Registered { name: String },
    Unregistered { name: String },
    Reloaded { name: String },
}

/// Tool call context for before/after hooks.
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub tool_name: String,
    pub session_id: String,
}
