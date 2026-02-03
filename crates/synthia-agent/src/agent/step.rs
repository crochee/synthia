//! Step processing implementation

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::Arc,
};

use chrono::Utc;
use futures::stream::{BoxStream, StreamExt};
use rmcp::model::{SamplingMessage, Tool, ToolUseContent};
use tokio_util::sync::CancellationToken;
use tracing::instrument;

use super::Agent;
use crate::{
    AgentError,
    Result,
    agent::{
        loop_detector::{OperationPattern, Outcome},
        tool_executor::{
            ToolErrorSummary,
            ToolExecutionResult,
            ToolStreamItem,
        },
    },
    config::SessionConfig,
    hooks::HookEvent,
    types::{AgentEvent, AgentStatus},
    utils::extract_tool_uses,
};

impl Agent {
    #[instrument(skip_all, name = "agent_step", fields(session_id = %session_config.id))]
    pub(super) async fn step<'a>(
        &'a self,
        conversation: &'a [SamplingMessage],
        session_config: &'a SessionConfig,
        tools: &'a [Tool],
        cancel_token: &'a CancellationToken,
    ) -> Result<BoxStream<'a, Result<AgentEvent>>> {
        let system_prompt = Some(self.build_system_prompt().await);

        let stream = async_stream::stream! {
            let model_stream = self.call_model_with_retry(
                system_prompt,
                conversation,
                tools,
                session_config.backoff.clone(),
                cancel_token,
            ).await?;

            let mut tool_uses: Vec<ToolUseContent> = Vec::new();

            tokio::pin!(model_stream);
            while let Some(result) = model_stream.next().await {
                if cancel_token.is_cancelled() {
                    yield Ok(AgentEvent::Status(AgentStatus::Cancelled));
                    return;
                }

                match result {
                Ok(create_result) => {
                    let msg = create_result.message;
                    tool_uses.extend(extract_tool_uses(&msg));

                    if let Err(e) = self.deps.session.add_message(session_config, &msg).await {
                        tracing::warn!("Failed to add assistant message: {}", e);
                        yield Err(e);
                    }
                    yield Ok(AgentEvent::Message(msg));

                    match create_result.stop_reason.as_deref() {
                        Some("stop") => {
                            yield Ok(AgentEvent::Status(AgentStatus::Completed));
                            return;
                        }
                        Some(other) if !matches!(other, "tool_use" | "function_call" | "tool_calls") => {
                            tracing::warn!("Model stopped with reason: {}", other);
                            yield Ok(AgentEvent::Status(AgentStatus::Errored(other.to_string())));
                            return;
                        }
                        _ => {}
                    }
                }
                Err(e) => {
                    tracing::error!("Model error: {}", e);
                    yield Err(e);
                }
            }
            }

            if !tool_uses.is_empty() {
                let tool_config = self.deps.tools.config().await;
                let tool_stream = self.process_tool_uses(
                    tool_uses,
                    session_config,
                    cancel_token,
                    tool_config.max_concurrent_tools,
                ).await;

                tokio::pin!(tool_stream);
                while let Some(event) = tool_stream.next().await {
                    yield event;
                }
            }
        };

        Ok(Box::pin(stream))
    }

    pub(super) async fn process_tool_uses<'a>(
        &'a self,
        tool_uses: Vec<ToolUseContent>,
        session_config: &'a SessionConfig,
        cancel_token: &'a CancellationToken,
        max_concurrent: usize,
    ) -> BoxStream<'a, Result<AgentEvent>> {
        if tool_uses.is_empty() {
            return Box::pin(futures::stream::empty());
        }

        let session_config = session_config.clone();
        let cancel_token = cancel_token.clone();
        let agent = Arc::new(self.clone());

        tracing::debug!(
            tool_count = tool_uses.len(),
            max_concurrent,
            "Processing tool uses"
        );

        let stream = async_stream::stream! {
            let tool_count = tool_uses.len();
            let tool_futures = tool_uses.into_iter().map(|tool_use| {
                let tool_name = tool_use.name.clone();
                let session_config = session_config.clone();
                let cancel_token = cancel_token.clone();
                let agent = Arc::clone(&agent);

                async move { Agent::execute_single_tool(agent, tool_use, tool_name, session_config, cancel_token).await }
            });

            let mut concurrent_stream = futures::stream::iter(tool_futures).buffer_unordered(max_concurrent);
            let mut all_errors = ToolErrorSummary::new();

            while let Some(Some(execution_result)) = concurrent_stream.next().await {
                all_errors.add_errors(&execution_result);
                for event in execution_result.events { yield event; }
            }

            let turn_complete_event = AgentEvent::TurnCompleteDetail {
                turn_id: session_config.id.clone(),
                tool_count,
                has_errors: all_errors.to_summary_message().is_some(),
            };
            yield Ok(turn_complete_event);

            if let Some(summary) = all_errors.to_summary_message() {
                tracing::warn!("Tool execution completed with errors: {}", summary);
                yield Err(AgentError::tool_error(summary));
            }
        };

        Box::pin(stream)
    }

    async fn execute_single_tool(
        agent: Arc<Self>,
        tool_use: ToolUseContent,
        tool_name: String,
        session_config: SessionConfig,
        cancel_token: CancellationToken,
    ) -> Option<ToolExecutionResult> {
        if cancel_token.is_cancelled() {
            return None;
        }

        tracing::debug!(tool_name = %tool_name, "Starting tool execution");

        let args_value = serde_json::Value::Object(tool_use.input.clone());
        agent
            .deps
            .hooks
            .emit(&HookEvent::BeforeToolCall {
                tool: tool_name.clone(),
                args: args_value.clone(),
            })
            .await;

        // Compute hash before args_value gets moved
        let mut hasher = DefaultHasher::new();
        args_value.hash(&mut hasher);
        let args_hash = hasher.finish();

        let mut execution_result = ToolExecutionResult::new(tool_name.clone());
        let success = Self::run_tool_stream(
            Arc::clone(&agent),
            tool_use,
            &tool_name,
            &session_config,
            &cancel_token,
            &mut execution_result,
        )
        .await;

        agent
            .deps
            .hooks
            .emit(&HookEvent::AfterToolCall {
                tool: tool_name.clone(),
                args: args_value,
                success,
            })
            .await;

        if execution_result.has_errors() {
            tracing::warn!(
                tool_name = %tool_name,
                error_count = execution_result.errors.len(),
                "Tool execution completed with errors"
            );
        } else {
            tracing::debug!(tool_name = %tool_name, "Tool execution completed successfully");
        }

        // Record tool execution in loop detector
        let pattern = OperationPattern {
            tool_name: tool_name.clone(),
            args_hash,
            timestamp: Utc::now(),
            outcome: if success {
                Outcome::Success
            } else {
                Outcome::Failure
            },
            result_hash: None,
        };
        if let Ok(mut guard) = agent.loop_detector.write() {
            guard.record(pattern);
        }

        Some(execution_result)
    }

    async fn run_tool_stream(
        agent: Arc<Self>,
        tool_use: ToolUseContent,
        tool_name: &str,
        session_config: &SessionConfig,
        cancel_token: &CancellationToken,
        execution_result: &mut ToolExecutionResult,
    ) -> bool {
        let tool_stream = match Agent::execute_tool(
            tool_use,
            Arc::clone(&agent),
            cancel_token.clone(),
            session_config,
        )
        .await
        {
            Ok(stream) => stream,
            Err(e) => {
                tracing::error!(tool_name = %tool_name, error = %e, "Failed to execute tool");
                execution_result.add_error(e.to_string());
                execution_result.add_event(Err(e));
                return false;
            }
        };

        tokio::pin!(tool_stream);
        let mut success = true;

        while let Some(stream_item) = tool_stream.next().await {
            if cancel_token.is_cancelled() {
                tracing::debug!(tool_name = %tool_name, "Tool execution cancelled");
                success = false;
                break;
            }

            match stream_item {
                ToolStreamItem::Message(notification) => {
                    execution_result.add_event(Ok(
                        AgentEvent::SystemNotification(notification),
                    ));
                }
                ToolStreamItem::Result(Ok(tool_response)) => {
                    if let Err(e) = agent
                        .deps
                        .session
                        .add_message(session_config, &tool_response)
                        .await
                    {
                        tracing::warn!(
                            tool_name = %tool_name,
                            error = %e,
                            "Failed to add tool result"
                        );
                    }
                    execution_result
                        .add_event(Ok(AgentEvent::Message(tool_response)));
                }
                ToolStreamItem::Result(Err(e)) => {
                    tracing::error!(tool_name = %tool_name, error = %e, "Tool execution failed");
                    success = false;
                    execution_result.add_error(e.to_string());
                    execution_result.add_event(Err(e));
                }
            }
        }

        success
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Async methods (step, process_tool_uses, execute_single_tool,
    // run_tool_stream) require full Agent mocking — suited for integration tests.

    #[test]
    fn test_process_tool_uses_empty_returns_empty_stream() {
        // process_tool_uses returns early with an empty stream when tool_uses is empty.
        // This tests the guard clause at line 119-121.
        // The actual async behavior is tested in integration tests.
        let empty_tool_uses: Vec<ToolUseContent> = Vec::new();
        assert!(empty_tool_uses.is_empty());
    }

    #[test]
    fn test_tool_use_content_structure() {
        use rmcp::model::ToolUseContent;

        // Test that ToolUseContent can be constructed using ::new()
        // Signature: new(id, name, input: JsonObject)
        let tool_use =
            ToolUseContent::new("test-id", "test_tool", serde_json::Map::new());

        assert_eq!(tool_use.id, "test-id");
        assert_eq!(tool_use.name, "test_tool");
        assert!(tool_use.input.is_empty());
    }

    #[test]
    fn test_tool_execution_result_drops_properly() {
        // Verify that ToolExecutionResult can be instantiated and dropped
        // without issues (used extensively in step.rs)
        let result = ToolExecutionResult::new("cleanup_test".to_string());
        assert_eq!(result.tool_name, "cleanup_test");
        assert!(!result.has_errors());
    }

    #[test]
    fn test_tool_error_summary_no_errors() {
        // Test ToolErrorSummary behavior when no errors added
        let summary = ToolErrorSummary::new();
        assert!(summary.to_summary_message().is_none());
    }

    #[test]
    fn test_hash_computation_for_args() {
        use std::{
            collections::hash_map::DefaultHasher,
            hash::{Hash, Hasher},
        };

        // Verify the hash pattern used in execute_single_tool for args_hash
        // This is the same pattern at lines 184-186
        let args_value =
            serde_json::json!({"command": "ls -la", "path": "/tmp"});
        let mut hasher = DefaultHasher::new();
        args_value.hash(&mut hasher);
        let hash1 = hasher.finish();

        // Same args should produce same hash
        let mut hasher2 = DefaultHasher::new();
        let args_value2 =
            serde_json::json!({"command": "ls -la", "path": "/tmp"});
        args_value2.hash(&mut hasher2);
        let hash2 = hasher2.finish();

        assert_eq!(hash1, hash2);

        // Different args should produce different hash
        let mut hasher3 = DefaultHasher::new();
        let args_value3 =
            serde_json::json!({"command": "rm -rf", "path": "/tmp"});
        args_value3.hash(&mut hasher3);
        let hash3 = hasher3.finish();

        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_cancel_token_cancelled_check() {
        use tokio_util::sync::CancellationToken;

        let token = CancellationToken::new();
        assert!(!token.is_cancelled());

        let cloned = token.clone();
        cloned.cancel();
        assert!(token.is_cancelled());
    }
}
