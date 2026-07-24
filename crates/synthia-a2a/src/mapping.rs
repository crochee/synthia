//! AgentEvent → A2A StreamResponse mapping.
//!
//! Translates the agent's internal [`AgentEvent`] stream into A2A
//! protocol [`StreamResponse`]s so `SynthiaExecutor` can directly emit
//! A2A-compatible streaming events.
//!
//! # Phase-5 Wire Format
//!
//! Every content-carrying `AgentEvent` variant is translated into a
//! `Part::data(serde_json::Value)` carrying an object whose `"kind"`
//! key discriminates the variant and whose remaining keys carry the
//! payload verbatim. The frontend (Phase 7) dispatches on
//! `JSON.parse(part.data).kind`.
//!
//! Lifecycle `SystemEvent::Session*` variants keep `StreamResponse::StatusUpdate`
//! shape (those are not `Part::data`). All other `SystemEvent`s and
//! `HookEvent`s become `Message(Agent)` carrying a `Part::data`.
//!
//! See `openspec/changes/simplify-agent-event-stream/specs/agent-event-bus/spec.md`
//! for the canonical wire format.

use a2a::{
    Message,
    Part,
    Role,
    StreamResponse,
    Task,
    TaskId,
    TaskState,
    TaskStatus,
    TaskStatusUpdateEvent,
    new_message_id,
};
use synthia_agent::{
    AgentEvent,
    ContentPart,
    events::{HookEvent, SessionEndReason, SystemEvent, WarningKind},
};
use synthia_provider::{ReasoningContent, TextContent, ToolUse};

/// Convert a single [`AgentEvent`] to zero or more A2A [`StreamResponse`]s.
///
/// Mapping rules (Phase 5):
/// - `System::SessionStarted` → `StatusUpdate(Working)`
/// - `System::SessionEnded(_)` → `StatusUpdate(<derived state>)`
/// - `System::SessionInterrupted` → `StatusUpdate(InputRequired)`
/// - `ModelDone` → `Message(Agent)` with `Part::data({ kind: "response_complete", ... })`
/// - `Model(ContentPart::*)` → `Message(Agent)` with `Part::data({ kind, ...payload })`
/// - `System::Progress` → `Message(Agent)` with `Part::data({ kind: "progress", ... })`
/// - `System::Warning` → `Message(Agent)` with `Part::data({ kind: "warning", ... })`
/// - `System::Recovery` → `Message(Agent)` with `Part::data({ kind: "recovery", ... })`
/// - `System::Usage` → `Message(Agent)` with `Part::data({ kind: "usage", ... })`
/// - `Hook(HookEvent::*)` → `Message(Agent)` with `Part::data({ kind, ...payload })`
/// - `Agent(meta, inner)` → emits two responses: a meta
///   `Part::data({ kind: "agent_meta", ... })` followed by the
///   recursive translation of `inner`.
pub fn agent_event_to_stream_responses(
    event: &AgentEvent,
    task_id: &TaskId,
    context_id: &str,
) -> Vec<Result<StreamResponse, a2a::A2AError>> {
    match event {
        AgentEvent::System(SystemEvent::SessionStarted { .. }) => {
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

        AgentEvent::System(SystemEvent::SessionEnded { reason }) => {
            let state = session_end_reason_to_task_state(reason);
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

        AgentEvent::System(SystemEvent::SessionInterrupted { .. }) => {
            vec![Ok(StreamResponse::StatusUpdate(TaskStatusUpdateEvent {
                task_id: task_id.clone(),
                context_id: context_id.to_string(),
                status: TaskStatus {
                    state: TaskState::InputRequired,
                    message: None,
                    timestamp: None,
                },
                metadata: None,
            }))]
        }

        AgentEvent::System(SystemEvent::Progress {
            message,
            step,
            total,
        }) => vec![Ok(StreamResponse::Message(message_with_data_part(
            task_id,
            context_id,
            serde_json::json!({
                "kind": "progress",
                "message": message,
                "step": step,
                "total": total,
            }),
        )))],

        AgentEvent::System(SystemEvent::Warning {
            kind,
            message,
            iteration,
        }) => {
            let source = warning_kind_to_source(kind);
            vec![Ok(StreamResponse::Message(message_with_data_part(
                task_id,
                context_id,
                serde_json::json!({
                    "kind": "warning",
                    "source": source,
                    "message": message,
                    "iteration": iteration,
                }),
            )))]
        }

        AgentEvent::System(SystemEvent::Recovery {
            level_number,
            tool_name,
            message,
            iteration,
        }) => vec![Ok(StreamResponse::Message(message_with_data_part(
            task_id,
            context_id,
            serde_json::json!({
                "kind": "recovery",
                "level": level_number,
                "tool_name": tool_name,
                "message": message,
                "iteration": iteration,
            }),
        )))],

        AgentEvent::System(SystemEvent::Usage {
            input_tokens,
            output_tokens,
            cache_read_tokens,
            cache_creation_tokens,
        }) => vec![Ok(StreamResponse::Message(message_with_data_part(
            task_id,
            context_id,
            serde_json::json!({
                "kind": "usage",
                "input_tokens": input_tokens,
                "output_tokens": output_tokens,
                "cache_read_tokens": cache_read_tokens,
                "cache_creation_tokens": cache_creation_tokens,
            }),
        )))],

        AgentEvent::ModelDone(sampling) => {
            let value = serde_json::to_value(sampling)
                .unwrap_or_else(|_| serde_json::json!({}));
            // Wrap so the top-level shape is { kind: "response_complete", <sampling fields> }.
            let mut obj = serde_json::Map::new();
            obj.insert(
                "kind".to_string(),
                serde_json::Value::String("response_complete".to_string()),
            );
            if let serde_json::Value::Object(inner) = value {
                for (k, v) in inner {
                    obj.insert(k, v);
                }
            }
            vec![Ok(StreamResponse::Message(message_with_data_part(
                task_id,
                context_id,
                serde_json::Value::Object(obj),
            )))]
        }

        AgentEvent::Model(part) => {
            vec![Ok(StreamResponse::Message(message_with_data_part(
                task_id,
                context_id,
                model_part_to_data_value(part),
            )))]
        }

        AgentEvent::Hook(hook) => {
            vec![Ok(StreamResponse::Message(message_with_data_part(
                task_id,
                context_id,
                hook_event_to_data_value(hook),
            )))]
        }

        AgentEvent::Agent(meta, inner) => {
            let mut out = Vec::with_capacity(2);
            out.push(Ok(StreamResponse::Message(message_with_data_part(
                task_id,
                context_id,
                serde_json::json!({
                    "kind": "agent_meta",
                    "parent_session_id": meta.parent_session_id,
                    "child_session_id": meta.child_session_id,
                    "parent_depth": meta.parent_depth,
                }),
            ))));
            out.extend(agent_event_to_stream_responses(
                inner, task_id, context_id,
            ));
            out
        }
    }
}

/// Project a [`SessionEndReason`] to its A2A [`TaskState`].
///
/// Mirrors the spec scenario "StatusUpdate state is derived from
/// SessionEvent".
fn session_end_reason_to_task_state(reason: &SessionEndReason) -> TaskState {
    match reason {
        SessionEndReason::Completed => TaskState::Completed,
        SessionEndReason::Cancelled => TaskState::Canceled,
        SessionEndReason::Error(_) => TaskState::Failed,
        SessionEndReason::TokenBudgetExceeded => TaskState::Failed,
        SessionEndReason::MaxIterationsReached => TaskState::Failed,
        SessionEndReason::GuardianBlocked => TaskState::Rejected,
        SessionEndReason::LoopDetected => TaskState::Failed,
        SessionEndReason::CircuitBreakerOpen => TaskState::Failed,
    }
}

/// Build an `Agent`-role [`Message`] carrying a single
/// [`Part::data`] with the supplied JSON value.
fn message_with_data_part(
    task_id: &TaskId,
    context_id: &str,
    data: serde_json::Value,
) -> Message {
    Message {
        message_id: new_message_id(),
        context_id: Some(context_id.to_string()),
        task_id: Some(task_id.clone()),
        role: Role::Agent,
        parts: vec![Part::data(data)],
        metadata: None,
        extensions: None,
        reference_task_ids: None,
    }
}

/// Translate a [`ContentPart`] into its wire `Part::data` payload.
fn model_part_to_data_value(part: &ContentPart) -> serde_json::Value {
    match part {
        ContentPart::Text(TextContent {
            text,
            cache_control,
        }) => {
            serde_json::json!({
                "kind": "model_text",
                "text": text,
                "cache_control": cache_control,
            })
        }
        ContentPart::Reasoning(ReasoningContent { text, signature }) => {
            serde_json::json!({
                "kind": "model_reasoning",
                "text": text,
                "signature": signature,
            })
        }
        ContentPart::ToolUse(ToolUse { id, name, input }) => {
            serde_json::json!({
                "kind": "tool_call",
                "tool_use_id": id,
                "tool_name": name,
                "input": input,
            })
        }
        ContentPart::ToolResult(tr) => {
            let content: Vec<serde_json::Value> = tr
                .content
                .iter()
                .filter_map(|c| serde_json::to_value(c).ok())
                .collect();
            serde_json::json!({
                "kind": "tool_result",
                "tool_use_id": tr.tool_use_id,
                "content": content,
                "structured_content": tr.structured_content,
                "is_error": tr.is_error,
            })
        }
        ContentPart::Image(image) => {
            serde_json::json!({
                "kind": "model_image",
                "data": image.data,
                "mime_type": image.mime_type,
                "detail": image.detail,
            })
        }
        ContentPart::Audio(audio) => {
            serde_json::json!({
                "kind": "model_audio",
                "data": audio.data,
                "mime_type": audio.mime_type,
                "format": audio.format,
            })
        }
        ContentPart::Resource(resource) => {
            serde_json::json!({
                "kind": "model_resource",
                "uri": resource.uri,
                "name": resource.name,
                "title": resource.title,
                "description": resource.description,
                "mime_type": resource.mime_type,
            })
        }
    }
}

/// Translate a [`HookEvent`] into its wire `Part::data` payload.
fn hook_event_to_data_value(hook: &HookEvent) -> serde_json::Value {
    match hook {
        HookEvent::Message { priority, message } => {
            serde_json::json!({
                "kind": "steering_message",
                "priority": priority,
                "message": message,
            })
        }
        HookEvent::ConfirmRequest {
            tool_use_id,
            tool_name,
            reason,
        } => {
            serde_json::json!({
                "kind": "guardian_confirm_request",
                "tool_use_id": tool_use_id,
                "tool_name": tool_name,
                "reason": reason,
            })
        }
        HookEvent::ConfirmResponse {
            approved,
            tool_use_id,
        } => {
            serde_json::json!({
                "kind": "guardian_confirm_response",
                "approved": approved,
                "tool_use_id": tool_use_id,
            })
        }
        HookEvent::Custom { kind, data } => {
            serde_json::json!({
                "kind": "custom",
                "custom_kind": kind,
                "data": data,
            })
        }
    }
}

/// Render a [`WarningKind`] as the wire `source` string.
fn warning_kind_to_source(kind: &WarningKind) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| "unknown".to_string())
}

/// Extract the first text part from an A2A `Message`.
///
/// Used to translate an A2A `SendMessageRequest.message` to a
/// Synthia prompt text.
pub fn extract_text_from_message(msg: &Message) -> Option<String> {
    msg.parts.iter().find_map(|p| {
        if let a2a::PartContent::Text(t) = &p.content {
            Some(t.clone())
        } else {
            None
        }
    })
}

/// Build a final [`Task`] with the given state (used for the cancel path).
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

/// Extract the `Part::data` JSON value from a single-part
/// `Message(Agent)` produced by [`agent_event_to_stream_responses`].
///
/// Returns `None` if the message has zero parts, more than one part,
/// or the single part is not a `Part::data` payload.
#[cfg(test)]
fn first_data_value(msg: &Message) -> Option<serde_json::Value> {
    let part = msg.parts.first()?;
    if let a2a::PartContent::Data(v) = &part.content {
        Some(v.clone())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use synthia_agent::events::{AgentMeta, SessionEndReason, WarningKind};
    use synthia_provider::{
        ContentPart,
        ImageContent,
        ImageDetail,
        ReasoningContent,
        ResourceLink,
        SamplingResult,
        TextContent,
        TokenUsage,
        ToolResult,
        ToolUse,
        types::{AudioContent, AudioFormat},
    };

    use super::*;

    fn test_ids() -> (TaskId, String) {
        ("task-1".to_string(), "ctx-1".to_string())
    }

    fn empty_sampling() -> SamplingResult {
        SamplingResult {
            text: String::new(),
            tool_calls: vec![],
            reasoning: String::new(),
            reasoning_signature: None,
            usage: TokenUsage::default(),
        }
    }

    #[test]
    fn session_started_maps_to_working() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::System(SystemEvent::SessionStarted {
            session_id: "s1".to_string(),
        });
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        assert_eq!(results.len(), 1);
        let resp = results[0].as_ref().unwrap();
        match resp {
            StreamResponse::StatusUpdate(su) => {
                assert_eq!(su.status.state, TaskState::Working);
                assert_eq!(su.task_id, "task-1");
            }
            other => panic!("expected StatusUpdate, got {other:?}"),
        }
    }

    #[test]
    fn session_ended_clean_maps_to_completed() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::System(SystemEvent::SessionEnded {
            reason: SessionEndReason::Completed,
        });
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        match results[0].as_ref().unwrap() {
            StreamResponse::StatusUpdate(su) => {
                assert_eq!(su.status.state, TaskState::Completed);
            }
            other => panic!("expected StatusUpdate, got {other:?}"),
        }
    }

    #[test]
    fn session_ended_cancelled_maps_to_canceled() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::System(SystemEvent::SessionEnded {
            reason: SessionEndReason::Cancelled,
        });
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        match results[0].as_ref().unwrap() {
            StreamResponse::StatusUpdate(su) => {
                assert_eq!(su.status.state, TaskState::Canceled);
            }
            other => panic!("expected StatusUpdate, got {other:?}"),
        }
    }

    #[test]
    fn session_ended_error_maps_to_failed() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::System(SystemEvent::SessionEnded {
            reason: SessionEndReason::Error("oops".to_string()),
        });
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        match results[0].as_ref().unwrap() {
            StreamResponse::StatusUpdate(su) => {
                assert_eq!(su.status.state, TaskState::Failed);
            }
            other => panic!("expected StatusUpdate, got {other:?}"),
        }
    }

    #[test]
    fn session_interrupted_maps_to_input_required() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::System(SystemEvent::SessionInterrupted {
            reason: "user cancel".to_string(),
        });
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        match results[0].as_ref().unwrap() {
            StreamResponse::StatusUpdate(su) => {
                assert_eq!(su.status.state, TaskState::InputRequired);
            }
            other => panic!("expected StatusUpdate, got {other:?}"),
        }
    }

    #[test]
    fn model_done_maps_to_data_part() {
        let (tid, cid) = test_ids();
        let mut sampling = empty_sampling();
        sampling.text = "Hello world".to_string();
        let event = AgentEvent::ModelDone(sampling);
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        assert_eq!(results.len(), 1);
        match results[0].as_ref().unwrap() {
            StreamResponse::Message(msg) => {
                assert_eq!(msg.role, Role::Agent);
                let data = first_data_value(msg).expect("data part");
                assert_eq!(data["kind"], "response_complete");
                assert_eq!(data["text"], "Hello world");
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[test]
    fn model_text_maps_to_data_part() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::Model(ContentPart::Text(TextContent {
            text: "hi".to_string(),
            cache_control: None,
        }));
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        match results[0].as_ref().unwrap() {
            StreamResponse::Message(msg) => {
                let data = first_data_value(msg).expect("data part");
                assert_eq!(data["kind"], "model_text");
                assert_eq!(data["text"], "hi");
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[test]
    fn model_reasoning_maps_to_data_part() {
        let (tid, cid) = test_ids();
        let event =
            AgentEvent::Model(ContentPart::Reasoning(ReasoningContent {
                text: "hmm".to_string(),
                signature: None,
            }));
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        match results[0].as_ref().unwrap() {
            StreamResponse::Message(msg) => {
                let data = first_data_value(msg).expect("data part");
                assert_eq!(data["kind"], "model_reasoning");
                assert_eq!(data["text"], "hmm");
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[test]
    fn model_tool_use_maps_to_data_part() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::Model(ContentPart::ToolUse(ToolUse {
            id: "u1".to_string(),
            name: "read_file".to_string(),
            input: serde_json::json!({"path": "/tmp/test"}),
        }));
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        match results[0].as_ref().unwrap() {
            StreamResponse::Message(msg) => {
                let data = first_data_value(msg).expect("data part");
                assert_eq!(data["kind"], "tool_call");
                assert_eq!(data["tool_use_id"], "u1");
                assert_eq!(data["tool_name"], "read_file");
                assert_eq!(
                    data["input"],
                    serde_json::json!({"path": "/tmp/test"})
                );
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[test]
    fn model_tool_result_maps_to_data_part() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::Model(ContentPart::ToolResult(
            ToolResult::new("u1", "file contents"),
        ));
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        match results[0].as_ref().unwrap() {
            StreamResponse::Message(msg) => {
                let data = first_data_value(msg).expect("data part");
                assert_eq!(data["kind"], "tool_result");
                assert_eq!(data["tool_use_id"], "u1");
                assert!(data["content"].is_array());
                assert_eq!(data["content"][0]["text"], "file contents");
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[test]
    fn model_image_maps_to_data_part() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::Model(ContentPart::Image(ImageContent {
            data: "b64-data".to_string(),
            mime_type: "image/png".to_string(),
            detail: Some(ImageDetail::High),
        }));
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        match results[0].as_ref().unwrap() {
            StreamResponse::Message(msg) => {
                let data = first_data_value(msg).expect("data part");
                assert_eq!(data["kind"], "model_image");
                assert_eq!(data["data"], "b64-data");
                assert_eq!(data["mime_type"], "image/png");
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[test]
    fn model_audio_maps_to_data_part() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::Model(ContentPart::Audio(AudioContent {
            data: "b64-audio".to_string(),
            mime_type: "audio/wav".to_string(),
            format: Some(AudioFormat::Wav),
        }));
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        match results[0].as_ref().unwrap() {
            StreamResponse::Message(msg) => {
                let data = first_data_value(msg).expect("data part");
                assert_eq!(data["kind"], "model_audio");
                assert_eq!(data["data"], "b64-audio");
                assert_eq!(data["mime_type"], "audio/wav");
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[test]
    fn model_resource_maps_to_data_part() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::Model(ContentPart::Resource(ResourceLink {
            uri: "file:///x.txt".to_string(),
            name: "x.txt".to_string(),
            title: Some("X".to_string()),
            description: Some("desc".to_string()),
            mime_type: Some("text/plain".to_string()),
        }));
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        match results[0].as_ref().unwrap() {
            StreamResponse::Message(msg) => {
                let data = first_data_value(msg).expect("data part");
                assert_eq!(data["kind"], "model_resource");
                assert_eq!(data["uri"], "file:///x.txt");
                assert_eq!(data["name"], "x.txt");
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[test]
    fn system_progress_maps_to_data_part() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::System(SystemEvent::Progress {
            message: "working".to_string(),
            step: 1,
            total: 10,
        });
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        match results[0].as_ref().unwrap() {
            StreamResponse::Message(msg) => {
                let data = first_data_value(msg).expect("data part");
                assert_eq!(data["kind"], "progress");
                assert_eq!(data["message"], "working");
                assert_eq!(data["step"], 1);
                assert_eq!(data["total"], 10);
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[test]
    fn warning_maps_to_data_part() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::System(SystemEvent::Warning {
            kind: WarningKind::Guardian,
            message: "x".to_string(),
            iteration: Some(1),
        });
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        match results[0].as_ref().unwrap() {
            StreamResponse::Message(msg) => {
                let data = first_data_value(msg).expect("data part");
                assert_eq!(data["kind"], "warning");
                assert_eq!(data["source"], "guardian");
                assert_eq!(data["message"], "x");
                assert_eq!(data["iteration"], 1);
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[test]
    fn recovery_maps_to_data_part() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::System(SystemEvent::Recovery {
            level_number: 1,
            tool_name: Some("bash".to_string()),
            message: "truncated".to_string(),
            iteration: Some(3),
        });
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        match results[0].as_ref().unwrap() {
            StreamResponse::Message(msg) => {
                let data = first_data_value(msg).expect("data part");
                assert_eq!(data["kind"], "recovery");
                assert_eq!(data["level"], 1);
                assert_eq!(data["tool_name"], "bash");
                assert_eq!(data["message"], "truncated");
                assert_eq!(data["iteration"], 3);
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[test]
    fn usage_maps_to_data_part() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::System(SystemEvent::Usage {
            input_tokens: 10,
            output_tokens: 20,
            cache_read_tokens: Some(5),
            cache_creation_tokens: None,
        });
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        match results[0].as_ref().unwrap() {
            StreamResponse::Message(msg) => {
                let data = first_data_value(msg).expect("data part");
                assert_eq!(data["kind"], "usage");
                assert_eq!(data["input_tokens"], 10);
                assert_eq!(data["output_tokens"], 20);
                assert_eq!(data["cache_read_tokens"], 5);
                assert!(data["cache_creation_tokens"].is_null());
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[test]
    fn hook_message_maps_to_data_part() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::Hook(HookEvent::Message {
            priority: 7,
            message: "steer".to_string(),
        });
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        match results[0].as_ref().unwrap() {
            StreamResponse::Message(msg) => {
                let data = first_data_value(msg).expect("data part");
                assert_eq!(data["kind"], "steering_message");
                assert_eq!(data["priority"], 7);
                assert_eq!(data["message"], "steer");
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[test]
    fn hook_confirm_request_maps_to_data_part() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::Hook(HookEvent::ConfirmRequest {
            tool_use_id: "u1".to_string(),
            tool_name: "bash".to_string(),
            reason: "needs approval".to_string(),
        });
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        match results[0].as_ref().unwrap() {
            StreamResponse::Message(msg) => {
                let data = first_data_value(msg).expect("data part");
                assert_eq!(data["kind"], "guardian_confirm_request");
                assert_eq!(data["tool_use_id"], "u1");
                assert_eq!(data["tool_name"], "bash");
                assert_eq!(data["reason"], "needs approval");
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[test]
    fn hook_confirm_response_maps_to_data_part() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::Hook(HookEvent::ConfirmResponse {
            approved: true,
            tool_use_id: "u1".to_string(),
        });
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        match results[0].as_ref().unwrap() {
            StreamResponse::Message(msg) => {
                let data = first_data_value(msg).expect("data part");
                assert_eq!(data["kind"], "guardian_confirm_response");
                assert_eq!(data["approved"], true);
                assert_eq!(data["tool_use_id"], "u1");
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[test]
    fn hook_custom_maps_to_data_part() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::Hook(HookEvent::Custom {
            kind: "my_ext.event".to_string(),
            data: serde_json::json!({"hello": "world"}),
        });
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        match results[0].as_ref().unwrap() {
            StreamResponse::Message(msg) => {
                let data = first_data_value(msg).expect("data part");
                assert_eq!(data["kind"], "custom");
                assert_eq!(data["custom_kind"], "my_ext.event");
                assert_eq!(data["data"], serde_json::json!({"hello": "world"}));
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[test]
    fn agent_meta_emits_two_stream_responses() {
        let (tid, cid) = test_ids();
        let meta = AgentMeta::new("parent-1", "child-1", 1);
        let inner = AgentEvent::Model(ContentPart::Text(TextContent {
            text: "x".to_string(),
            cache_control: None,
        }));
        let event = AgentEvent::Agent(meta, Box::new(inner));
        let results = agent_event_to_stream_responses(&event, &tid, &cid);
        assert_eq!(results.len(), 2);

        // First: agent_meta data part.
        match results[0].as_ref().unwrap() {
            StreamResponse::Message(msg) => {
                let data = first_data_value(msg).expect("data part");
                assert_eq!(data["kind"], "agent_meta");
                assert_eq!(data["parent_session_id"], "parent-1");
                assert_eq!(data["child_session_id"], "child-1");
                assert_eq!(data["parent_depth"], 1);
            }
            other => panic!("expected Message(meta), got {other:?}"),
        }

        // Second: the inner Model(Text) translation.
        match results[1].as_ref().unwrap() {
            StreamResponse::Message(msg) => {
                let data = first_data_value(msg).expect("data part");
                assert_eq!(data["kind"], "model_text");
                assert_eq!(data["text"], "x");
            }
            other => panic!("expected Message(inner), got {other:?}"),
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
