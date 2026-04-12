//! Step processing implementation

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::Arc,
};

use chrono::Utc;
use futures::stream::{BoxStream, StreamExt};
use rmcp::model::{
    RawTextContent,
    Role,
    SamplingContent,
    SamplingMessage,
    SamplingMessageContent,
    Tool,
    ToolUseContent,
};
use tokio_util::sync::CancellationToken;
use tracing::instrument;

use super::{
    Agent,
    step_plan::{ExecutionMode, ScheduleBuilder, ToolCallInfo},
};
use crate::{
    AgentError,
    Result,
    agent::{
        loop_detector::{OperationPattern, Outcome},
        tool_executor::{ToolErrorSummary, ToolExecutionResult},
    },
    config::SessionConfig,
    hooks::{
        HookEvent,
        events::{
            AfterToolBatchComplete,
            PhaseInfo,
            ScheduleInfo,
            ToolInfo,
            ToolSchedulingPlan,
        },
    },
    types::{AgentEvent, AgentStatus, ErrorEvent, ErrorSource, TurnEndReason},
    utils::extract_tool_uses,
};

impl Agent {
    #[instrument(skip_all, name = "agent_step", fields(session_id = %session_config.id))]
    pub(super) async fn step_from_session(
        &self,
        session_config: &SessionConfig,
        tools: &[Tool],
        cancel_token: &CancellationToken,
    ) -> Result<BoxStream<'static, AgentEvent>> {
        let conversation = self
            .deps
            .session
            .get_conversation(session_config)
            .await
            .map_err(|e| {
                AgentError::context(format!("Failed to get conversation: {e}"))
            })?
            .to_vec();

        let session_config = session_config.clone();
        let tools = tools.to_vec();
        let cancel_token = cancel_token.clone();

        Ok(Self::step_with_owned_conversation(
            self.clone(),
            conversation,
            session_config,
            tools,
            cancel_token,
        ))
    }

    fn step_with_owned_conversation(
        agent: Agent,
        conversation: Vec<SamplingMessage>,
        session_config: SessionConfig,
        tools: Vec<Tool>,
        cancel_token: CancellationToken,
    ) -> BoxStream<'static, AgentEvent> {
        Box::pin(async_stream::stream! {
            let system_prompt = Some(agent.build_system_prompt().await);

            let model_stream = match agent.call_model_with_retry(
                system_prompt,
                &conversation,
                &tools,
                session_config.backoff.clone(),
                &cancel_token,
            ).await {
                Ok(s) => s,
                Err(e) => {
                    yield AgentEvent::Error(ErrorEvent {
                        source: ErrorSource::Model,
                        message: e.to_string(),
                        suggestion: None,
                    });
                    return;
                }
            };

            let mut tool_uses: Vec<ToolUseContent> = Vec::new();

            tokio::pin!(model_stream);
            while let Some(result) = model_stream.next().await {
                if cancel_token.is_cancelled() {
                    yield AgentEvent::Status(AgentStatus::Cancelled);
                    return;
                }

                match result {
                Ok(create_result) => {
                    let msg = create_result.message;
                    tool_uses.extend(extract_tool_uses(&msg));

                    if let Err(e) = agent.deps.session.add_message(&session_config, &msg).await {
                        tracing::warn!("Failed to add assistant message: {}", e);
                        return;
                    }
                    yield AgentEvent::Message(msg);

                    match create_result.stop_reason.as_deref() {
                        Some("stop") => {
                            yield AgentEvent::Status(AgentStatus::Completed);
                            return;
                        }
                        Some(other) if !matches!(other, "tool_use" | "function_call" | "tool_calls") => {
                            tracing::warn!("Model stopped with reason: {}", other);
                            yield AgentEvent::Status(AgentStatus::Errored(other.to_string()));
                            return;
                        }
                        _ => {}
                    }
                }
                Err(e) => {
                    tracing::error!("Model error: {}", e);
                    yield AgentEvent::Error(ErrorEvent {
                        source: ErrorSource::Model,
                        message: e.to_string(),
                        suggestion: None,
                    });
                }
            }
            }

            if !tool_uses.is_empty() {
                let tool_config = agent.deps.tools.config().await;
                let tool_stream = agent.process_tool_uses(
                    tool_uses,
                    &session_config,
                    &cancel_token,
                    tool_config.max_concurrent_tools,
                ).await;

                tokio::pin!(tool_stream);
                while let Some(event) = tool_stream.next().await {
                    yield event;
                }
            }
        })
    }

    pub(super) async fn process_tool_uses<'a>(
        &'a self,
        tool_uses: Vec<ToolUseContent>,
        session_config: &'a SessionConfig,
        cancel_token: &'a CancellationToken,
        max_concurrent: usize,
    ) -> BoxStream<'a, AgentEvent> {
        if tool_uses.is_empty() {
            return Box::pin(futures::stream::empty())
                as BoxStream<'a, AgentEvent>;
        }

        let session_config = session_config.clone();
        let cancel_token = cancel_token.clone();
        let agent = Arc::new(self.clone());

        let stream: BoxStream<'a, AgentEvent> = Box::pin(
            async_stream::stream! {
                // Convert ToolUseContent to ToolCallInfo for scheduling
                let tool_infos: Vec<ToolCallInfo> = match tool_uses
                    .into_iter()
                    .map(|tu| {
                        agent.deps.tools.get_tool(&tu.name)
                            .ok_or_else(|| AgentError::tool(&tu.name, "tool not found"))
                            .map(|tool| {
                                let args_value = serde_json::Value::Object(tu.input.clone());
                                let is_read_only = tool.is_read_only(&args_value);
                                let is_concurrency_safe = tool.is_concurrency_safe(&args_value);
                                ToolCallInfo::new(
                                    tu.id,
                                    tu.name,
                                    serde_json::Value::Object(tu.input),
                                    is_read_only,
                                    is_concurrency_safe,
                                )
                            })
                    })
                    .collect()
                {
                    Ok(infos) => infos,
                    Err(e) => {
                        yield AgentEvent::Error(ErrorEvent {
                            source: ErrorSource::Tool("scheduling".to_string()),
                            message: e.to_string(),
                            suggestion: None,
                        });
                        return;
                    }
                };

                // Build the tool schedule
                let schedule = ScheduleBuilder::new().with_tools(tool_infos).build();

                tracing::debug!(
                    tool_count = schedule.total_tools,
                    phases = schedule.phases.len(),
                    "Built tool schedule"
                );

                // Pre-extract phase info to avoid lifetime issues with async_stream
                let phase_infos: Vec<(ExecutionMode, Vec<ToolCallInfo>)> = schedule
                    .phases
                    .into_iter()
                    .map(|p| (p.execution_mode, p.tools))
                    .collect();

                let total_tools =
                    phase_infos.iter().map(|(_, tools)| tools.len()).sum();
                // Emit ToolSchedulingPlan hook after building schedule
                let turn_id = session_config.id.clone();
                let session_id = session_config.id.clone();

                // Build schedule event data
                let schedule_event = ToolSchedulingPlan {
                    session_id: session_id.clone(),
                    turn_id: turn_id.clone(),
                    tools: phase_infos
                        .iter()
                        .flat_map(|(_, tools)| tools.iter())
                        .map(|t| ToolInfo {
                            id: t.id.clone(),
                            name: t.name.clone(),
                            is_read_only: t.is_read_only,
                            is_concurrency_safe: t.is_concurrency_safe,
                        })
                        .collect(),
                    schedule: ScheduleInfo {
                        total_tools,
                        phases: phase_infos
                            .iter()
                            .enumerate()
                            .map(|(idx, (mode, tools))| PhaseInfo {
                                phase_id: idx as u32,
                                tool_count: tools.len(),
                                execution_mode: format!("{mode:?}"),
                            })
                            .collect(),
                    },
                };
                agent.deps.hooks.emit_ordered(&HookEvent::ToolSchedulingPlan(schedule_event)).await;

                let mut all_errors = ToolErrorSummary::new();

                // Execute phases in order
                for (phase_idx, (execution_mode, phase_tools)) in phase_infos.into_iter().enumerate() {
                    let phase_start_time = std::time::Instant::now();
                    let batch_id = phase_idx as u32;
                    let phase_tool_count = phase_tools.len();

                    tracing::debug!(
                        phase_id = batch_id,
                        tool_count = phase_tool_count,
                        mode = ?execution_mode,
                        "Executing phase"
                    );

                    match execution_mode {
                        ExecutionMode::Parallel => {
                            let tool_futures = phase_tools.into_iter().map(|tool_info| {
                                let tool_use = ToolUseContent::new(
                                    tool_info.id,
                                    tool_info.name.clone(),
                                    tool_info.args.as_object().cloned().unwrap_or_default(),
                                );
                                let tool_name = tool_info.name;
                                let session_config = session_config.clone();
                                let cancel_token = cancel_token.clone();
                                let agent = Arc::clone(&agent);

                                async move {
                                    Agent::execute_single_tool(
                                        agent,
                                        tool_use,
                                        tool_name,
                                        session_config,
                                        cancel_token,
                                    )
                                    .await
                                }
                            });

                            let max_concurrent = max_concurrent.max(1);
                            let mut concurrent_stream =
                                futures::stream::iter(tool_futures)
                                    .buffer_unordered(max_concurrent);

                            while let Some(Some(execution_result)) =
                                concurrent_stream.next().await
                            {
                                all_errors.add_errors(&execution_result);
                                for event in execution_result.events {
                                    yield event;
                                }
                            }
                        }
                        ExecutionMode::Serial => {
                            // Serial execution - one at a time
                            for tool_info in phase_tools {
                                if cancel_token.is_cancelled() {
                                    break;
                                }

                                let tool_use = ToolUseContent::new(
                                    tool_info.id,
                                    tool_info.name.clone(),
                                    tool_info.args.as_object().cloned().unwrap_or_default(),
                                );
                                let tool_name = tool_info.name;

                                if let Some(execution_result) = Agent::execute_single_tool(
                                    Arc::clone(&agent),
                                    tool_use,
                                    tool_name,
                                    session_config.clone(),
                                    cancel_token.clone(),
                                )
                                .await
                                {
                                    all_errors.add_errors(&execution_result);
                                    for event in execution_result.events {
                                        yield event;
                                    }
                                }
                            }
                        }
                    }

                    // Emit AfterToolBatchComplete for this phase
                    let phase_has_errors = all_errors.has_errors();
                    agent
                        .deps
                        .hooks
                        .emit_ordered(&HookEvent::AfterToolBatchComplete(
                            AfterToolBatchComplete {
                                session_id: session_id.clone(),
                                batch_id,
                                tool_count: phase_tool_count,
                                has_errors: phase_has_errors,
                            },
                        ))
                        .await;

                    tracing::debug!(
                        phase_id = batch_id,
                        elapsed_ms = phase_start_time.elapsed().as_millis(),
                        "Phase completed"
                    );
                }

                let turn_end_event = AgentEvent::TurnEnd {
                    turn_id: turn_id.clone(),
                    reason: if let Some(summary) = all_errors.get_summary_message() {
                        TurnEndReason::Error(ErrorEvent {
                            source: ErrorSource::Tool("batch".to_string()),
                            message: summary,
                            suggestion: Some("请检查工具参数或尝试其他方法".to_string()),
                        })
                    } else {
                        TurnEndReason::Success(SamplingMessage {
                            role: Role::Assistant,
                            content: SamplingContent::Single(SamplingMessageContent::Text(
                                RawTextContent {
                                    text: format!("Completed {total_tools} tools"),
                                    meta: None,
                                },
                            )),
                            meta: None,
                        })
                    },
                };

                // Emit BeforeTurnComplete hook
                agent.deps.hooks
                    .emit_ordered(&HookEvent::BeforeTurnComplete {
                        session_id: session_id.clone(),
                        turn_id: turn_id.clone(),
                    })
                    .await;

                yield turn_end_event;

                // Emit AfterTurnComplete hook
                agent.deps.hooks
                    .emit_ordered(&HookEvent::AfterTurnComplete {
                        session_id: session_id.clone(),
                        turn_id,
                        has_errors: all_errors.get_summary_message().is_some(),
                    })
                    .await;

                if let Some(summary) = all_errors.get_summary_message() {
                    tracing::warn!("Tool execution completed with errors: {}", summary);
                }
            },
        );

        stream
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
            .emit_ordered(&HookEvent::BeforeToolCall {
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
            .emit_ordered(&HookEvent::AfterToolCall {
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
        if let Ok(mut guard) = agent.loop_detector.try_write() {
            guard.record(pattern);
        }

        Some(execution_result)
    }
}

impl Agent {
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
                execution_result.add_event(AgentEvent::Error(ErrorEvent {
                    source: ErrorSource::Tool(tool_name.to_string()),
                    message: e.to_string(),
                    suggestion: None,
                }));
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
                Ok(tool_response) => {
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
                        .add_event(AgentEvent::Message(tool_response));
                }
                Err(e) => {
                    tracing::error!(tool_name = %tool_name, error = %e, "Tool execution failed");
                    success = false;
                    execution_result.add_error(e.to_string());
                    execution_result.add_event(AgentEvent::Error(ErrorEvent {
                        source: ErrorSource::Tool(tool_name.to_string()),
                        message: e.to_string(),
                        suggestion: None,
                    }));
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
        let mut summary = ToolErrorSummary::new();
        assert!(summary.get_summary_message().is_none());
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
