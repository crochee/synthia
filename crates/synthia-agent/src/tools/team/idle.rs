//! Idle tool implementation

use async_trait::async_trait;
use rmcp::model::{CallToolResult, Content};
use serde_json::Value;

use crate::tools::Tool;

/// Signals that the agent has no more work and should enter idle state.
#[derive(Clone)]
pub(crate) struct IdleTool;

#[async_trait]
impl Tool for IdleTool {
    fn name(&self) -> &str {
        "idle"
    }

    fn description(&self) -> &str {
        "Signal that the agent has no more work and should enter idle state. Use when current task is complete."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn call(&self, _args: Value) -> CallToolResult {
        CallToolResult::success(vec![Content::text(
            "Agent entered idle state. Will wait for new messages or tasks.",
        )])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_idle_tool_name() {
        let tool = IdleTool;
        assert_eq!(tool.name(), "idle");
    }

    #[tokio::test]
    async fn test_idle_tool_description() {
        let tool = IdleTool;
        assert!(tool.description().contains("idle"));
    }

    #[tokio::test]
    async fn test_idle_tool_parameters() {
        let tool = IdleTool;
        let params = tool.parameters();
        assert_eq!(params["type"], "object");
    }

    #[tokio::test]
    async fn test_idle_tool_call() {
        let tool = IdleTool;
        let result = tool.call(serde_json::json!({})).await;
        assert!(result.is_error.is_none() || result.is_error == Some(false));
        let content = &result.content[0];
        let text = content.as_text().unwrap();
        assert!(text.text.contains("idle"));
    }

    #[tokio::test]
    async fn test_idle_tool_clone() {
        let tool = IdleTool;
        let _cloned = tool;
    }
}
