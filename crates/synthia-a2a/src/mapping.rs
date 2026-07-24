//! AgentEvent → A2A StreamResponse 类型映射。
//!
//! 将 synthia 内部事件流转换为 A2A 协议的 `StreamResponse`，
//! 使得 `SynthiaExecutor` 可以直接输出 A2A 兼容的流式事件。

use a2a::{
    Artifact,
    Message,
    Part,
    Role,
    StreamResponse,
    Task,
    TaskArtifactUpdateEvent,
    TaskId,
    TaskState,
    TaskStatus,
    TaskStatusUpdateEvent,
    new_artifact_id,
    new_message_id,
};
use synthia_agent::AgentEvent;

/// 将一个 `AgentEvent` 转换为零个或多个 A2A `StreamResponse`。
///
/// 映射规则：
/// - `SessionStarted` → `StatusUpdate(Working)`
/// - `SessionEnded(Clean)` → `StatusUpdate(Completed)`
/// - `SessionEnded(Cancelled)` → `StatusUpdate(Canceled)`
/// - `SessionEnded(Error)` → `StatusUpdate(Failed)`
/// - `SessionInterrupted` → `StatusUpdate(Canceled)`
/// - `LlmResponseComplete` → `Message(Agent)` + `StatusUpdate(Working)`
/// - `LlmError` → `StatusUpdate(Failed)`
/// - `ToolCallCompleted` → `ArtifactUpdate`
/// - `Finish` → `Message(Agent)` + `StatusUpdate(Completed)`
/// - 其他事件 → 忽略（产生空 Vec）
pub fn agent_event_to_stream_responses(
    event: &AgentEvent,
    task_id: &TaskId,
    context_id: &str,
) -> Vec<Result<StreamResponse, a2a::A2AError>> {
    match event {
        AgentEvent::SessionStarted { .. } => {
            vec![Ok(StreamResponse::StatusUpdate(TaskStatusUpdateEvent {
                task_id: task_id.clone(),
                context_id: context_id.to_string(),
                status: TaskStatus {
                    state: TaskState::Working,
                    message: None,
                    timestamp: None,
                },
                metadata: None,
            }))]
        }

        AgentEvent::SessionEnded { reason } => {
            let state = match reason {
                synthia_agent::events::SessionEndReason::Completed => TaskState::Completed,
                synthia_agent::events::SessionEndReason::Cancelled => TaskState::Canceled,
                synthia_agent::events::SessionEndReason::Error(_) => TaskState::Failed,
                synthia_agent::events::SessionEndReason::TokenBudgetExceeded => TaskState::Failed,
                synthia_agent::events::SessionEndReason::MaxIterationsReached => TaskState::Failed,
                synthia_agent::events::SessionEndReason::GuardianBlocked => TaskState::Rejected,
                synthia_agent::events::SessionEndReason::LoopDetected => TaskState::Failed,
                synthia_agent::events::SessionEndReason::CircuitBreakerOpen => TaskState::Failed,
            };
            vec![Ok(StreamResponse::StatusUpdate(TaskStatusUpdateEvent {
                task_id: task_id.clone(),
                context_id: context_id.to_string(),
                status: TaskStatus {
                    state,
                    message: None,
                    timestamp: None,
                },
                metadata: None,
            }))]
        }

        AgentEvent::SessionInterrupted { .. } => {
            vec![Ok(StreamResponse::StatusUpdate(TaskStatusUpdateEvent {
                task_id: task_id.clone(),
                context_id: context_id.to_string(),
                status: TaskStatus {
                    state: TaskState::Canceled,
                    message: None,
                    timestamp: None,
                },
                metadata: None,
            }))]
        }

        AgentEvent::LlmResponseComplete { content, .. } => {
            let msg = Message {
                message_id: new_message_id(),
                context_id: Some(context_id.to_string()),
                task_id: Some(task_id.clone()),
                role: Role::Agent,
                parts: vec![Part::text(content.clone())],
                metadata: None,
                extensions: None,
                reference_task_ids: None,
            };
            vec![Ok(StreamResponse::Message(msg))]
        }

        AgentEvent::LlmError { error } => {
            let msg = Message {
                message_id: new_message_id(),
                context_id: Some(context_id.to_string()),
                task_id: Some(task_id.clone()),
                role: Role::Agent,
                parts: vec![Part::text(error.clone())],
                metadata: None,
                extensions: None,
                reference_task_ids: None,
            };
            vec![Ok(StreamResponse::StatusUpdate(TaskStatusUpdateEvent {
                task_id: task_id.clone(),
                context_id: context_id.to_string(),
                status: TaskStatus {
                    state: TaskState::Failed,
                    message: Some(msg),
                    timestamp: None,
                },
                metadata: None,
            }))]
        }

        AgentEvent::ToolCallCompleted {
            tool_name, output, ..
        } => {
            let artifact = Artifact {
                artifact_id: new_artifact_id(),
                name: Some(tool_name.clone()),
                description: None,
                parts: vec![Part::text(output.clone())],
                metadata: None,
                extensions: None,
            };
            vec![Ok(StreamResponse::ArtifactUpdate(
                TaskArtifactUpdateEvent {
                    task_id: task_id.clone(),
                    context_id: context_id.to_string(),
                    artifact,
                    append: Some(false),
                    last_chunk: Some(true),
                    metadata: None,
                },
            ))]
        }

        AgentEvent::Finish { output } => {
            let msg = Message {
                message_id: new_message_id(),
                context_id: Some(context_id.to_string()),
                task_id: Some(task_id.clone()),
                role: Role::Agent,
                parts: vec![Part::text(output.clone())],
                metadata: None,
                extensions: None,
                reference_task_ids: None,
            };
            vec![
                Ok(StreamResponse::Message(msg)),
                Ok(StreamResponse::StatusUpdate(TaskStatusUpdateEvent {
                    task_id: task_id.clone(),
                    context_id: context_id.to_string(),
                    status: TaskStatus {
                        state: TaskState::Completed,
                        message: None,
                        timestamp: None,
                    },
                    metadata: None,
                })),
            ]
        }

        AgentEvent::Thinking { text, iteration } => {
            let msg = Message {
                message_id: new_message_id(),
                context_id: Some(context_id.to_string()),
                task_id: Some(task_id.clone()),
                role: Role::Agent,
                parts: vec![Part::text(text.clone())],
                metadata: Some(std::collections::HashMap::from([
                    ("segment_type".to_string(), serde_json::json!("thinking")),
                    ("iteration".to_string(), serde_json::json!(iteration)),
                ])),
                extensions: None,
                reference_task_ids: None,
            };
            vec![Ok(StreamResponse::Message(msg))]
        }

        AgentEvent::ToolCallStarted { tool_name, input } => {
            let input_json = serde_json::to_string(input).unwrap_or_default();
            let msg = Message {
                message_id: new_message_id(),
                context_id: Some(context_id.to_string()),
                task_id: Some(task_id.clone()),
                role: Role::Agent,
                parts: vec![Part::text(input_json)],
                metadata: Some(std::collections::HashMap::from([
                    (
                        "segment_type".to_string(),
                        serde_json::json!("tool_call"),
                    ),
                    ("tool_name".to_string(), serde_json::json!(tool_name)),
                ])),
                extensions: None,
                reference_task_ids: None,
            };
            vec![Ok(StreamResponse::Message(msg))]
        }

        AgentEvent::LlmStreamDelta { content } => {
            let msg = Message {
                message_id: new_message_id(),
                context_id: Some(context_id.to_string()),
                task_id: Some(task_id.clone()),
                role: Role::Agent,
                parts: vec![Part::text(content.clone())],
                metadata: Some(std::collections::HashMap::from([(
                    "segment_type".to_string(),
                    serde_json::json!("text_delta"),
                )])),
                extensions: None,
                reference_task_ids: None,
            };
            vec![Ok(StreamResponse::Message(msg))]
        }

        AgentEvent::Progress {
            message,
            step,
            total,
        } => {
            let msg = Message {
                message_id: new_message_id(),
                context_id: Some(context_id.to_string()),
                task_id: Some(task_id.clone()),
                role: Role::Agent,
                parts: vec![Part::text(message.clone())],
                metadata: Some(std::collections::HashMap::from([
                    ("segment_type".to_string(), serde_json::json!("progress")),
                    ("step".to_string(), serde_json::json!(step)),
                    ("total".to_string(), serde_json::json!(total)),
                ])),
                extensions: None,
                reference_task_ids: None,
            };
            vec![Ok(StreamResponse::Message(msg))]
        }

        // 所有其他事件：忽略（不影响 A2A 流）
        _ => Vec::new(),
    }
}

/// 从 A2A `Message` 中提取第一个文本部分。
///
/// 用于将 A2A 的 `SendMessageRequest.message` 转换为 Synthia prompt 文本。
pub fn extract_text_from_message(msg: &Message) -> Option<String> {
    msg.parts.iter().find_map(|p| {
        if let a2a::PartContent::Text(t) = &p.content {
            Some(t.clone())
        } else {
            None
        }
    })
}

/// 从 A2A Task 构建最终状态响应（用于 cancel 路径）。
pub fn task_with_state(
    task_id: TaskId,
    context_id: String,
    state: TaskState,
    message: Option<Message>,
) -> Task {
    Task {
        id: task_id,
        context_id,
        status: TaskStatus {
            state,
            message,
            timestamp: None,
        },
        artifacts: None,
        history: None,
        metadata: None,
    }
}

#[cfg(test)]
mod tests {
    use synthia_agent::events::SessionEndReason;

    use super::*;

    fn test_ids() -> (TaskId, String) {
        ("task-1".to_string(), "ctx-1".to_string())
    }

    #[test]
    fn session_started_maps_to_working() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::SessionStarted {
            session_id: "s1".to_string(),
        };
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        assert_eq!(results.len(), 1);
        let resp = results[0].as_ref().unwrap();
        match resp {
            StreamResponse::StatusUpdate(su) => {
                assert_eq!(su.status.state, TaskState::Working);
                assert_eq!(su.task_id, "task-1");
            }
            _ => panic!("expected StatusUpdate, got {resp:?}"),
        }
    }

    #[test]
    fn session_ended_clean_maps_to_completed() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::SessionEnded {
            reason: SessionEndReason::Completed,
        };
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        assert_eq!(results.len(), 1);
        match results[0].as_ref().unwrap() {
            StreamResponse::StatusUpdate(su) => {
                assert_eq!(su.status.state, TaskState::Completed);
            }
            _ => panic!("expected StatusUpdate"),
        }
    }

    #[test]
    fn session_ended_cancelled_maps_to_canceled() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::SessionEnded {
            reason: SessionEndReason::Cancelled,
        };
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        assert_eq!(results.len(), 1);
        match results[0].as_ref().unwrap() {
            StreamResponse::StatusUpdate(su) => {
                assert_eq!(su.status.state, TaskState::Canceled);
            }
            _ => panic!("expected StatusUpdate"),
        }
    }

    #[test]
    fn session_ended_error_maps_to_failed() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::SessionEnded {
            reason: SessionEndReason::Error("oops".to_string()),
        };
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        assert_eq!(results.len(), 1);
        match results[0].as_ref().unwrap() {
            StreamResponse::StatusUpdate(su) => {
                assert_eq!(su.status.state, TaskState::Failed);
            }
            _ => panic!("expected StatusUpdate"),
        }
    }

    #[test]
    fn session_interrupted_maps_to_canceled() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::SessionInterrupted {
            reason: "user cancel".to_string(),
        };
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        assert_eq!(results.len(), 1);
        match results[0].as_ref().unwrap() {
            StreamResponse::StatusUpdate(su) => {
                assert_eq!(su.status.state, TaskState::Canceled);
            }
            _ => panic!("expected StatusUpdate"),
        }
    }

    #[test]
    fn llm_response_complete_maps_to_message() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::LlmResponseComplete {
            content: "Hello world".to_string(),
            usage: synthia_agent::events::TokenUsage {
                prompt_tokens: 10,
                completion_tokens: 20,
                total_tokens: 30,
                cached_prompt_tokens: None,
                cache_read_tokens: None,
                cache_write_tokens: None,
            },
        };
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        assert_eq!(results.len(), 1);
        match results[0].as_ref().unwrap() {
            StreamResponse::Message(msg) => {
                assert_eq!(msg.role, Role::Agent);
                assert_eq!(msg.text(), Some("Hello world"));
            }
            _ => panic!("expected Message"),
        }
    }

    #[test]
    fn llm_error_maps_to_failed_status() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::LlmError {
            error: "timeout".to_string(),
        };
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        assert_eq!(results.len(), 1);
        match results[0].as_ref().unwrap() {
            StreamResponse::StatusUpdate(su) => {
                assert_eq!(su.status.state, TaskState::Failed);
                assert!(su.status.message.is_some());
            }
            _ => panic!("expected StatusUpdate"),
        }
    }

    #[test]
    fn tool_call_completed_maps_to_artifact_update() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::ToolCallCompleted {
            tool_name: "read_file".to_string(),
            output: "file contents".to_string(),
            is_error: false,
        };
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        assert_eq!(results.len(), 1);
        match results[0].as_ref().unwrap() {
            StreamResponse::ArtifactUpdate(au) => {
                assert_eq!(au.artifact.name.as_deref(), Some("read_file"));
                assert_eq!(au.last_chunk, Some(true));
            }
            _ => panic!("expected ArtifactUpdate"),
        }
    }

    #[test]
    fn finish_maps_to_message_and_completed() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::Finish {
            output: "done".to_string(),
        };
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        assert_eq!(results.len(), 2);
        match results[0].as_ref().unwrap() {
            StreamResponse::Message(msg) => {
                assert_eq!(msg.text(), Some("done"));
            }
            _ => panic!("expected Message"),
        }
        match results[1].as_ref().unwrap() {
            StreamResponse::StatusUpdate(su) => {
                assert_eq!(su.status.state, TaskState::Completed);
            }
            _ => panic!("expected StatusUpdate"),
        }
    }

    #[test]
    fn ignored_events_produce_empty() {
        let (tid, cid) = test_ids();
        // 只有尚未实现映射的事件才会产生空数组
        let events = vec![AgentEvent::LlmReasoningDelta {
            delta: "thinking...".to_string(),
        }];
        for event in events {
            let results = agent_event_to_stream_responses(&event, &tid, &cid);
            assert!(results.is_empty(), "expected empty for {event:?}");
        }
    }

    #[test]
    fn thinking_event_maps_to_message() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::Thinking {
            text: "hmm".to_string(),
            iteration: 1,
        };
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        assert_eq!(results.len(), 1);
        match results[0].as_ref().unwrap() {
            StreamResponse::Message(msg) => {
                assert_eq!(msg.text(), Some("hmm"));
                assert_eq!(
                    msg.metadata.as_ref().unwrap().get("segment_type").unwrap(),
                    "thinking"
                );
                assert_eq!(
                    msg.metadata.as_ref().unwrap().get("iteration").unwrap(),
                    1
                );
            }
            _ => panic!("expected Message"),
        }
    }

    #[test]
    fn tool_call_started_event_maps_to_message() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::ToolCallStarted {
            tool_name: "read_file".to_string(),
            input: serde_json::json!({"path": "/tmp/test"}),
        };
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        assert_eq!(results.len(), 1);
        match results[0].as_ref().unwrap() {
            StreamResponse::Message(msg) => {
                let text = msg.text().unwrap();
                assert!(text.contains("path"));
                assert!(text.contains("/tmp/test"));
                assert_eq!(
                    msg.metadata.as_ref().unwrap().get("segment_type").unwrap(),
                    "tool_call"
                );
                assert_eq!(
                    msg.metadata.as_ref().unwrap().get("tool_name").unwrap(),
                    "read_file"
                );
            }
            _ => panic!("expected Message"),
        }
    }

    #[test]
    fn llm_stream_delta_event_maps_to_message() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::LlmStreamDelta {
            content: "hi".to_string(),
        };
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        assert_eq!(results.len(), 1);
        match results[0].as_ref().unwrap() {
            StreamResponse::Message(msg) => {
                assert_eq!(msg.text(), Some("hi"));
                assert_eq!(
                    msg.metadata.as_ref().unwrap().get("segment_type").unwrap(),
                    "text_delta"
                );
            }
            _ => panic!("expected Message"),
        }
    }

    #[test]
    fn progress_event_maps_to_message() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::Progress {
            message: "working".to_string(),
            step: 1,
            total: 10,
        };
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        assert_eq!(results.len(), 1);
        match results[0].as_ref().unwrap() {
            StreamResponse::Message(msg) => {
                assert_eq!(msg.text(), Some("working"));
                assert_eq!(
                    msg.metadata.as_ref().unwrap().get("segment_type").unwrap(),
                    "progress"
                );
                assert_eq!(
                    msg.metadata.as_ref().unwrap().get("step").unwrap(),
                    1
                );
                assert_eq!(
                    msg.metadata.as_ref().unwrap().get("total").unwrap(),
                    10
                );
            }
            _ => panic!("expected Message"),
        }
    }

    #[test]
    fn extract_text_from_message_returns_first_text() {
        let msg = Message::new(Role::User, vec![Part::text("hello")]);
        assert_eq!(extract_text_from_message(&msg), Some("hello".to_string()));
    }

    #[test]
    fn extract_text_from_message_returns_none_for_no_text() {
        let msg = Message::new(
            Role::User,
            vec![Part::data(serde_json::json!({"k": "v"}))],
        );
        assert_eq!(extract_text_from_message(&msg), None);
    }

    #[test]
    fn task_with_state_builds_correctly() {
        let task = task_with_state(
            "t1".to_string(),
            "c1".to_string(),
            TaskState::Canceled,
            None,
        );
        assert_eq!(task.id, "t1");
        assert_eq!(task.status.state, TaskState::Canceled);
    }
}
