//! Tool provider types and trait definition.

use async_trait::async_trait;

/// A lifecycle event delivered to a [`ToolProvider`] via [`ToolProvider::on_tool_event`].
///
/// These correspond to the `tool_*` hook points in the agent lifecycle.
#[derive(Debug, Clone)]
pub enum HookEvent {
    /// Fired before a tool is invoked.
    BeforeTool {
        tool_name: String,
        args: serde_json::Value,
    },
    /// Fired after a tool succeeds.
    AfterTool {
        tool_name: String,
        args: serde_json::Value,
        result: serde_json::Value,
    },
    /// Fired when a tool call fails.
    ToolError {
        tool_name: String,
        args: serde_json::Value,
        error: String,
    },
}

/// Result of a pre-execution check on a tool call.
#[derive(Debug, Clone)]
pub enum ToolPreCheck {
    /// Tool call is allowed to proceed immediately.
    Allow,
    /// Tool call requires user approval before execution.
    RequiresApproval,
    /// Tool call is denied; do not execute.
    Deny,
}

/// A reference to a JSON Schema for a tool's input parameters.
#[derive(Debug, Clone)]
pub enum SchemaRef {
    /// Inline schema value (owned).
    Inline(serde_json::Value),
    /// Reference to a schema by name (e.g. "#/definitions/MyToolInput").
    Ref(String),
}

/// Metadata about a single tool exposed by a provider.
#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: SchemaRef,
    pub deprecated: Option<String>,
}

#[async_trait]
pub trait ToolProvider: Send + Sync {
    /// Unique identifier for this provider.
    fn name(&self) -> &'static str;

    /// List all tools exposed by this provider.
    fn list_tools(&self) -> Vec<ToolDefinition>;

    /// Pre-execution check for a tool call.
    fn pre_check(&self, _tool_name: &str) -> ToolPreCheck {
        ToolPreCheck::Allow
    }

    /// Optional before-execute hook. Called before each tool execution.
    /// Return `Err(msg)` to deny execution.
    fn before_execute(
        &self,
        _tool_name: &str,
        _args: &serde_json::Value,
    ) -> Result<(), String> {
        Ok(())
    }

    /// Optional after-execute hook. Called after each tool execution.
    fn after_execute(
        &self,
        _tool_name: &str,
        _args: &serde_json::Value,
        _result: &serde_json::Value,
    ) {
    }

    /// Receive lifecycle events filtered to `tool_*` events.
    fn on_tool_event(&self, _event: &HookEvent) {}
}
