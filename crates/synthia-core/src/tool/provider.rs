//! ToolProvider trait — the registration contract for tools.

use std::sync::Arc;

use async_trait::async_trait;

use crate::tool::{
    descriptor::ToolDescriptor,
    types::{ToolError, ToolOutput},
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
    ///
    /// **Deprecated:** use `InterceptorChain` with `PermissionInterceptor` /
    /// `ApprovalInterceptor` at position 0 instead. The interceptor chain
    /// is the unified security guard path and cannot be bypassed.
    #[deprecated(
        since = "0.12.0",
        note = "Use InterceptorChain (synthia-agent::interceptor) instead; \
                before_execute is not guaranteed to run — the InterceptorChain \
                at position 0 is the authoritative guard."
    )]
    async fn before_execute(&self, _call: &ToolCall) -> Result<(), ToolError> {
        Ok(())
    }

    /// Post-execution hook. Default: no-op.
    ///
    /// **Deprecated:** use `InterceptorChain` with `TraceInterceptor` /
    /// `RetryInterceptor` for post-tool observability and retry logic.
    #[deprecated(
        since = "0.12.0",
        note = "Use InterceptorChain (synthia-agent::interceptor) instead; \
                after_execute is not guaranteed to run — the InterceptorChain \
                is the authoritative post-tool hook path."
    )]
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
