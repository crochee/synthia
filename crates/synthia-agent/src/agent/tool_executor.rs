//! Tool dispatcher module

use std::sync::Arc;

use futures::stream::BoxStream;
use rmcp::model::{
    CallToolResult,
    Content,
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
    guardian::{ApprovalRequest, ReviewDecision},
    types::{AgentEvent, SystemNotification, SystemNotificationType},
    utils::create_tool_message,
};

pub(crate) type ToolStream =
    BoxStream<'static, ToolStreamItem<Result<SamplingMessage>>>;

#[derive(Clone, Debug)]
pub(crate) enum ToolStreamItem<T> {
    Message(SystemNotification),
    Result(T),
}

pub(crate) struct ToolExecutionResult {
    pub(crate) tool_name: String,
    pub(crate) events: Vec<Result<AgentEvent>>,
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

    pub(crate) fn add_event(&mut self, event: Result<AgentEvent>) {
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
    pub(crate) fn build_guardian_request(
        &self,
        tool_name: &str,
        tool_args: serde_json::Value,
    ) -> Option<ApprovalRequest> {
        let cwd = self.config.workspace_dir.to_string_lossy().to_string();
        let id = uuid::Uuid::new_v4().to_string();

        if tool_name == "exec" || tool_name == "bash" || tool_name == "shell" {
            let command = tool_args
                .get("command")
                .and_then(|v| v.as_str())
                .map(|s| vec![s.to_string()])
                .unwrap_or_default();
            let justification = tool_args
                .get("justification")
                .and_then(|v| v.as_str())
                .map(ToString::to_string);
            return Some(ApprovalRequest::shell(
                id,
                command,
                cwd,
                justification,
            ));
        }

        if tool_name == "apply_patch" {
            let patch = tool_args
                .get("patch")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let files: Vec<String> = tool_args
                .get("files")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            return Some(ApprovalRequest::apply_patch(
                id,
                cwd,
                files,
                patch.lines().count(),
                patch.to_string(),
            ));
        }

        None
    }

    pub(crate) fn requires_guardian_review(&self, tool_name: &str) -> bool {
        self.deps.guardian.is_dangerous_tool(tool_name)
    }

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

    fn create_tool_error_response(
        tool_request_id: &str,
        error_messages: Vec<&str>,
    ) -> SamplingMessage {
        create_tool_message(
            tool_request_id.to_string(),
            CallToolResult::error(
                error_messages.into_iter().map(Content::text).collect(),
            ),
        )
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

        if let Some(result) = agent
            .run_guardian_review(
                &tool_name,
                &tool_args,
                &tool_request_id,
                &cancel_token,
            )
            .await
        {
            return result;
        }

        let tool_registry = Arc::clone(&agent.deps.tools);

        let args = if tool_args.is_null() {
            serde_json::Value::Object(serde_json::Map::new())
        } else {
            tool_args
        };

        Ok(Box::pin(futures::stream::once(async move {
            let exec_result: Result<CallToolResult, AgentError> = tool_registry
                .execute_with_tool(&tool_name, &args, |tool| {
                    let args = args.clone();
                    async move { tool.call(args).await }
                })
                .await;

            match exec_result {
                Ok(result) => ToolStreamItem::Result(Ok(create_tool_message(
                    tool_request_id,
                    result,
                ))),
                Err(e) => ToolStreamItem::Result(Err(e)),
            }
        })))
    }

    async fn run_guardian_review(
        &self,
        tool_name: &str,
        tool_args: &serde_json::Value,
        tool_request_id: &str,
        cancel_token: &CancellationToken,
    ) -> Option<Result<ToolStream>> {
        if !self.requires_guardian_review(tool_name) {
            return None;
        }

        let request =
            self.build_guardian_request(tool_name, tool_args.clone())?;
        tracing::info!(tool_name = %tool_name, "Running Guardian security review");

        match self.deps.guardian.review(cancel_token, request).await {
            Ok(Some(ReviewDecision::Approved)) => {
                tracing::info!(tool_name = %tool_name, "Guardian approved");
                None
            }
            Ok(Some(ReviewDecision::Denied { reason })) => {
                tracing::warn!(tool_name = %tool_name, reason = %reason, "Guardian denied");
                let response = Self::create_guardian_error_response(
                    tool_request_id,
                    &format!("Action blocked by Guardian: {reason}"),
                    "Please modify your request or provide additional justification.",
                );
                Some(Ok(Box::pin(futures::stream::once(
                    async move { response },
                ))))
            }
            Ok(Some(ReviewDecision::NeedsUserInput { question, options })) => {
                tracing::info!(tool_name = %tool_name, "Guardian requires user input for action");
                let question_data = serde_json::json!({
                    "question": question,
                    "options": options,
                });
                let notification = SystemNotification {
                    notification_type: SystemNotificationType::InlineMessage,
                    msg: question,
                    data: Some(question_data),
                };
                Some(Ok(Box::pin(futures::stream::once(async move {
                    ToolStreamItem::Message(notification)
                }))))
            }
            Ok(None) => {
                tracing::info!(tool_name = %tool_name, "Guardian review skipped");
                None
            }
            Err(e) => {
                tracing::error!(tool_name = %tool_name, error = %e, "Guardian review failed");
                let id = tool_request_id.to_string();
                Some(Ok(Box::pin(futures::stream::once(async move {
                    Self::create_guardian_error_response(
                        &id,
                        &format!("Guardian review failed: {e}"),
                        "Please try again later.",
                    )
                }))))
            }
        }
    }

    fn create_guardian_error_response(
        tool_request_id: &str,
        primary: &str,
        suggestion: &str,
    ) -> ToolStreamItem<Result<SamplingMessage>> {
        ToolStreamItem::Result(Ok(Self::create_tool_error_response(
            tool_request_id,
            vec![primary, suggestion],
        )))
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
    fn test_create_tool_error_response() {
        let response = Agent::create_tool_error_response(
            "tool-123",
            vec!["Error 1", "Error 2"],
        );

        assert_eq!(response.role, Role::User);
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
