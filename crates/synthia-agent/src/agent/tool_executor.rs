//! Tool dispatcher module

use std::sync::Arc;

use futures::stream::BoxStream;
use rmcp::model::{
    CallToolResult,
    Role,
    SamplingContent,
    SamplingMessage,
    SamplingMessageContent,
    ToolUseContent,
};
use tokio_util::sync::CancellationToken;
use tracing::instrument;

use crate::{
    AgentError,
    Result,
    agent::Agent,
    config::SessionConfig,
    types::AgentEvent,
    utils::create_tool_message,
};

pub(crate) type ToolStream =
    BoxStream<'static, Result<SamplingMessage, AgentError>>;

pub(crate) struct ToolExecutionResult {
    pub(crate) tool_name: String,
    pub(crate) events: Vec<AgentEvent>,
    pub(crate) errors: Vec<String>,
}

impl ToolExecutionResult {
    pub(crate) fn new(tool_name: String) -> Self {
        Self {
            tool_name,
            events: Vec::new(),
            errors: Vec::new(),
        }
    }

    pub(crate) fn add_event(&mut self, event: AgentEvent) {
        self.events.push(event);
    }

    pub(crate) fn add_error(&mut self, error: String) {
        self.errors.push(error);
    }

    pub(crate) fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

#[derive(Debug, Default)]
pub(crate) struct ToolErrorSummary {
    tool_name: Option<String>,
    error_count: usize,
    errors: Vec<String>,
    cached_summary: Option<String>,
}

impl ToolErrorSummary {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn add_errors(&mut self, result: &ToolExecutionResult) {
        if self.tool_name.is_none() {
            self.tool_name = Some(result.tool_name.clone());
        }
        self.error_count += result.errors.len();
        self.errors.extend(result.errors.iter().cloned());
        self.cached_summary = None;
    }

    pub(crate) fn get_summary_message(&mut self) -> Option<String> {
        if self.error_count == 0 {
            return None;
        }
        if let Some(ref cached) = self.cached_summary {
            return Some(cached.clone());
        }

        let tool_name = self.tool_name.as_deref().unwrap_or("unknown");
        let summary = if self.error_count == 1 {
            format!(
                "Tool '{}' encountered an error: {}",
                tool_name,
                self.errors
                    .first()
                    .map(String::as_str)
                    .unwrap_or("unknown error")
            )
        } else {
            format!(
                "Multiple tool errors occurred ({} total): {}",
                self.error_count,
                self.errors.join("; ")
            )
        };
        self.cached_summary = Some(summary.clone());
        Some(summary)
    }

    pub(crate) fn has_errors(&self) -> bool {
        self.error_count > 0
    }
}

impl Agent {
    pub fn create_error_response(
        error_message: String,
        detailed_error: Option<String>,
    ) -> SamplingMessage {
        let content = match detailed_error {
            Some(detailed) => SamplingContent::Multiple(vec![
                SamplingMessageContent::text(error_message),
                SamplingMessageContent::text(detailed),
            ]),
            None => SamplingContent::Single(SamplingMessageContent::text(
                error_message,
            )),
        };
        SamplingMessage {
            role: Role::Assistant,
            content,
            meta: None,
        }
    }

    #[instrument(skip_all)]
    pub(crate) async fn execute_tool(
        tool_use: ToolUseContent,
        agent: Arc<Agent>,
        cancel_token: CancellationToken,
        _session_config: &SessionConfig,
    ) -> Result<ToolStream> {
        let tool_request_id = tool_use.id.clone();
        let tool_name = tool_use.name.clone();
        let tool_args = serde_json::Value::Object(tool_use.input);

        let tool_registry = Arc::clone(&agent.deps.tools);

        let args = if tool_args.is_null() {
            serde_json::Value::Object(serde_json::Map::new())
        } else {
            tool_args
        };

        // Guardian review is now handled inside execute_with_tool
        Ok(Box::pin(futures::stream::once(async move {
            let exec_result: Result<CallToolResult, AgentError> = tool_registry
                .execute_with_tool(&tool_name, &args, &cancel_token)
                .await;

            match exec_result {
                Ok(result) => Ok(create_tool_message(tool_request_id, result)),
                Err(e) => Err(e),
            }
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_error_response_single() {
        let response =
            Agent::create_error_response("Error occurred".to_string(), None);

        assert_eq!(response.role, Role::Assistant);
        if let SamplingContent::Single(content) = &response.content {
            if let SamplingMessageContent::Text(text) = content {
                assert_eq!(text.text, "Error occurred");
            } else {
                panic!("Expected text content");
            }
        } else {
            panic!("Expected single content");
        }
    }

    #[test]
    fn test_create_error_response_multiple() {
        let response = Agent::create_error_response(
            "Error occurred".to_string(),
            Some("Details here".to_string()),
        );

        assert_eq!(response.role, Role::Assistant);
        if let SamplingContent::Multiple(contents) = &response.content {
            assert_eq!(contents.len(), 2);
        } else {
            panic!("Expected multiple content");
        }
    }

    #[test]
    fn test_tool_execution_result() {
        let mut result = ToolExecutionResult::new("test_tool".to_string());

        assert_eq!(result.tool_name, "test_tool");
        assert!(result.events.is_empty());
        assert!(result.errors.is_empty());
        assert!(!result.has_errors());

        result.add_error("Test error".to_string());
        assert!(result.has_errors());
        assert_eq!(result.errors.len(), 1);
    }

    #[test]
    fn test_tool_error_summary() {
        let mut summary = ToolErrorSummary::new();

        assert!(summary.get_summary_message().is_none());

        let result1 = ToolExecutionResult {
            tool_name: "tool1".to_string(),
            events: vec![],
            errors: vec!["First error".to_string()],
        };
        summary.add_errors(&result1);
        assert!(summary.get_summary_message().is_some());

        let msg = summary.get_summary_message();
        assert!(msg.is_some());
        assert!(msg.unwrap().contains("tool1"));

        let result2 = ToolExecutionResult {
            tool_name: "tool2".to_string(),
            events: vec![],
            errors: vec!["Second error".to_string()],
        };
        summary.add_errors(&result2);
        let msg = summary.get_summary_message();
        assert!(msg.is_some());
        assert!(msg.unwrap().contains("Multiple tool errors"));
    }

    #[test]
    fn test_tool_error_summary_single_error_with_empty_errors_list() {
        // Test the panic case when error_count == 1 but errors vec is empty
        // This should not panic due to index out of bounds on errors[0]
        let mut summary = ToolErrorSummary {
            tool_name: Some("test_tool".to_string()),
            error_count: 1, // indicates 1 error
            errors: vec![], // but errors list is empty
            cached_summary: None,
        };

        // This should handle the edge case gracefully without panicking
        let result = summary.get_summary_message();
        // When errors vec is empty but error_count > 0, it should still
        // avoid indexing into an empty vec
        if let Some(msg) = result {
            assert!(msg.contains("test_tool"));
        }
    }

    #[test]
    fn test_tool_error_summary_caching() {
        let mut summary = ToolErrorSummary::new();

        let result1 = ToolExecutionResult {
            tool_name: "tool1".to_string(),
            events: vec![],
            errors: vec!["First error".to_string()],
        };
        summary.add_errors(&result1);

        // First call computes and caches
        let msg1 = summary.get_summary_message();

        // Second call should return cached value
        let msg2 = summary.get_summary_message();

        assert_eq!(msg1, msg2);
    }

    #[test]
    fn test_tool_error_summary_multiple_errors_formats_correctly() {
        let mut summary = ToolErrorSummary::new();

        let result = ToolExecutionResult {
            tool_name: "multi_tool".to_string(),
            events: vec![],
            errors: vec![
                "Error A".to_string(),
                "Error B".to_string(),
                "Error C".to_string(),
            ],
        };
        summary.add_errors(&result);

        let msg = summary.get_summary_message().unwrap();
        assert!(msg.contains("3 total"));
        assert!(msg.contains("Error A"));
        assert!(msg.contains("Error B"));
        assert!(msg.contains("Error C"));
        assert!(msg.contains("Error A; Error B; Error C"));
    }
}
