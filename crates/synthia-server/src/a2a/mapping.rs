//! `AgentEvent` → A2A `StreamResponse` mapping.
//!
//! Translates the agent's internal [`AgentEvent`] stream into A2A
//! protocol [`StreamResponse`]s so [`super::executor::SynthiaExecutor`]
//! can directly emit A2A-compatible streaming events.
//!
//! # Channels (A2A v1.0 §3.7)
//!
//! Two channels ride in parallel:
//!
//! - **`Message` channel — communication turns**:
//!   - `ContentPart::Text`        → `Message(Agent, Part::text(text))`
//!   - `ContentPart::ToolUse`     → `Message(Agent, Part::data({id, name, input}))`
//!   - `ContentPart::ToolResult`  → `Message(Agent, Part::data({tool_use_id, content, is_error}))`
//!     — the natural provider shape; no synthetic `kind` discriminator.
//!
//! - **`Artifact` channel — tangible deliverables**:
//!   - `ContentPart::Resource(ResourceLink)` →
//!     `ArtifactUpdate(Artifact { Part::Url(uri), filename, media_type, metadata })`.
//!     `ResourceLink` is the agent's "tangible output" pointer
//!     (file URI, MCP-style resource reference); per A2A §3.7 it
//!     belongs on `Task.artifacts`, not in `Message` history. A
//!     stable `artifact_id` is derived from the URI so follow-up
//!     updates dedupe by id.
//!   - `ContentPart::Image`, `ContentPart::Audio` flow through
//!     the same `Artifact` channel as `Part::text` / `Part::Raw`
//!     with `media_type` set.
//!
//! The MVP keeps these two channels clean: tool control flow
//! only ever rides `Message`; tangible outputs only ever ride
//! `Artifact`. The same discipline is mirrored by
//! [`super::task_history::TaskHistoryBuilder`], which writes
//! `Message`s to `Task.history` and `Artifact`s to
//! `Task.artifacts`.
//!
//! # Dropped
//!
//! `System` (except `SessionStarted`, `ToolProgress(heartbeat)`,
//! `SessionEnded`), `ModelDone`, `Agent`, `Hook` events are
//! dropped at the A2A boundary — they have no native A2A
//! equivalent in MVP scope (lifecycle is communicated via
//! `StatusUpdate`).

use std::collections::HashMap;

use a2a::{
    Artifact,
    Message,
    Part,
    PartContent,
    Role,
    StreamResponse,
    Task,
    TaskArtifactUpdateEvent,
    TaskId,
    TaskState,
    TaskStatus,
    TaskStatusUpdateEvent,
    new_message_id,
};
use synthia_agent::{AgentEvent, SystemEvent, events::SessionEndReason};
use synthia_provider::{
    ContentPart,
    ResourceLink,
    ToolResult,
    ToolUse,
    types::{AudioContent, ImageContent},
};

/// Convert a single [`AgentEvent`] to zero or more A2A [`StreamResponse`]s.
pub fn agent_event_to_stream_responses(
    event: &AgentEvent,
    _sequence: u32,
    task_id: &TaskId,
    context_id: &str,
) -> Vec<Result<StreamResponse, a2a::A2AError>> {
    match event {
        AgentEvent::Model(part) => match part {
            // Communication turns — Message channel.
            ContentPart::Text(tc) => vec![Ok(StreamResponse::Message(
                text_message(task_id, context_id, &tc.text),
            ))],

            ContentPart::ToolUse(call) => {
                vec![Ok(StreamResponse::Message(message_with_data_part(
                    task_id,
                    context_id,
                    tool_use_to_data_value(call),
                )))]
            }

            ContentPart::ToolResult(tr) => {
                vec![Ok(StreamResponse::Message(message_with_data_part(
                    task_id,
                    context_id,
                    tool_result_to_data_value(tr),
                )))]
            }

            // Tangible deliverables — Artifact channel.
            //
            // `ResourceLink` is the canonical "this tool produced a
            // pointer to an external resource" carrier (MCP
            // ResourceLink, file URI, image CDN URL, etc.). Per
            // A2A v1.0 §3.7 it is a deliverable, not a chat
            // message, so it rides `StreamResponse::ArtifactUpdate`
            // and lands in `Task.artifacts`. See
            // `resource_link_to_artifact`.
            ContentPart::Resource(link) => {
                let artifact = resource_link_to_artifact(link);
                vec![Ok(StreamResponse::ArtifactUpdate(artifact_update(
                    task_id, context_id, artifact, false, true,
                )))]
            }

            ContentPart::Image(img) => {
                let artifact = image_to_artifact(img);
                vec![Ok(StreamResponse::ArtifactUpdate(artifact_update(
                    task_id, context_id, artifact, false, true,
                )))]
            }

            ContentPart::Audio(audio) => {
                let artifact = audio_to_artifact(audio);
                vec![Ok(StreamResponse::ArtifactUpdate(artifact_update(
                    task_id, context_id, artifact, false, true,
                )))]
            }

            ContentPart::Reasoning(_) => Vec::new(),
        },

        AgentEvent::System(SystemEvent::SessionStarted { .. }) => {
            // Initial submitted-task signal — emit a `Working`
            // status so A2A clients see the task enter the
            // working state.
            vec![Ok(StreamResponse::StatusUpdate(working_status_update(
                task_id, context_id,
            )))]
        }

        AgentEvent::System(SystemEvent::ToolProgress { tool_name, .. })
            if tool_name == "heartbeat" =>
        {
            // Heartbeat — keep the A2A stream emitting bytes
            // during long quiet phases (LLM thinking, long tool
            // runs) so intermediaries (nginx, enterprise proxies,
            // browser idle timers) don't silently close the
            // connection. The state stays `Working`.
            vec![Ok(StreamResponse::StatusUpdate(working_status_update(
                task_id, context_id,
            )))]
        }

        AgentEvent::System(SystemEvent::SessionEnded { reason }) => {
            let state =
                normalize_task_state(session_end_reason_to_task_state(reason));
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

        // Other System sub-variants, ModelDone, Agent, Hook — out of MVP scope.
        _ => Vec::new(),
    }
}

/// Build a `Working` [`TaskStatusUpdateEvent`] so A2A clients
/// see the task enter (or remain in) the working state. Used by
/// both the streaming path (heartbeat pings) and the executor's
/// session-start emit.
pub(crate) fn working_status_update(
    task_id: &TaskId,
    context_id: &str,
) -> TaskStatusUpdateEvent {
    TaskStatusUpdateEvent {
        task_id: task_id.clone(),
        context_id: context_id.to_string(),
        status: TaskStatus {
            state: TaskState::Working,
            message: None,
            timestamp: None,
        },
        metadata: None,
    }
}

/// Project a [`SessionEndReason`] to its A2A [`TaskState`].
fn session_end_reason_to_task_state(reason: &SessionEndReason) -> TaskState {
    match reason {
        SessionEndReason::Completed => TaskState::Completed,
        SessionEndReason::Cancelled => TaskState::Canceled,
        SessionEndReason::Error(_) => TaskState::Failed,
        SessionEndReason::MaxIterations => TaskState::Failed,
    }
}

/// Normalize a [`TaskState`] to one guaranteed to be in the
/// canonical `@a2a-js/sdk@1.0.0` enum set before it reaches the
/// wire.
fn normalize_task_state(state: TaskState) -> TaskState {
    match state {
        TaskState::Unspecified => {
            tracing::warn!(
                target: "synthia.a2a",
                "downgrading TaskState::Unspecified to TaskState::Failed"
            );
            TaskState::Failed
        }
        other => other,
    }
}

/// Build an `Agent`-role [`Message`] carrying a single [`Part::text`].
fn text_message(task_id: &TaskId, context_id: &str, text: &str) -> Message {
    Message {
        message_id: new_message_id(),
        context_id: Some(context_id.to_string()),
        task_id: Some(task_id.clone()),
        role: Role::Agent,
        parts: vec![Part::text(text.to_string())],
        metadata: None,
        extensions: None,
        reference_task_ids: None,
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

/// Build a [`TaskArtifactUpdateEvent`] for a complete artifact chunk.
///
/// `append` and `last_chunk` map 1:1 to the A2A wire fields.
/// For our MVP every artifact is emitted as one non-append,
/// last-chunk event; streaming accumulation (multiple
/// `append: true` chunks per artifact id) is a follow-up
/// concern. See [a2a.proto:308-322](file:///home/crochee/workspace/synthia/vendor/a2a-pb/proto/a2a.proto#L308)
/// for the wire contract.
fn artifact_update(
    task_id: &TaskId,
    context_id: &str,
    artifact: Artifact,
    append: bool,
    last_chunk: bool,
) -> TaskArtifactUpdateEvent {
    TaskArtifactUpdateEvent {
        task_id: task_id.clone(),
        context_id: context_id.to_string(),
        artifact,
        append: Some(append),
        last_chunk: Some(last_chunk),
        metadata: None,
    }
}

/// Translate a [`ResourceLink`] (the agent's "this tool produced
/// a pointer to an external resource" carrier) into an A2A
/// [`Artifact`].
///
/// Wire mapping per A2A v1.0 Part schema:
///
/// - `uri` → `Part::Url(uri)` (the SDK's URL-typed part
///   variant; there is no separate FilePart variant — `Url` is
///   the canonical "external resource reference" type).
/// - `name` → `artifact.name` (human-readable short name).
/// - `title` / `description` → `metadata` keys (the Part
///   schema only carries `filename` + `media_type` as
///   first-class metadata fields, so anything else rides in
///   `metadata` to stay forward-compat).
/// - `mime_type` → `Part::media_type`.
///
/// `artifact_id` is derived deterministically from the URI so
/// two `ResourceLink`s pointing at the same resource collapse
/// to the same artifact slot on the wire.
pub(crate) fn resource_link_to_artifact(link: &ResourceLink) -> Artifact {
    let mut metadata_map: HashMap<String, serde_json::Value> = HashMap::new();
    if let Some(title) = &link.title {
        metadata_map.insert(
            "title".to_string(),
            serde_json::Value::String(title.clone()),
        );
    }
    if let Some(desc) = &link.description {
        metadata_map.insert(
            "description".to_string(),
            serde_json::Value::String(desc.clone()),
        );
    }
    metadata_map.insert(
        "kind".to_string(),
        serde_json::Value::String("resource_link".to_string()),
    );

    let part = Part {
        content: PartContent::Url(link.uri.clone()),
        filename: Some(link.name.clone()),
        media_type: link.mime_type.clone(),
        metadata: if metadata_map.is_empty() {
            None
        } else {
            Some(metadata_map)
        },
    };

    Artifact {
        artifact_id: artifact_id_for_uri(&link.uri),
        name: Some(link.name.clone()),
        description: link.description.clone(),
        parts: vec![part],
        metadata: None,
        extensions: None,
    }
}

/// Derive a deterministic [`Artifact::artifact_id`] from a URI.
///
/// A2A requires `artifact_id` to be unique within a task. We
/// hash the URI so two `ResourceLink`s pointing at the same
/// resource collapse to the same artifact slot — letting a
/// future streaming update use `append: true` against the same
/// id. The hash is a `DefaultHasher` `u64` rendered as hex;
/// collision risk is acceptable here because the id is
/// task-scoped, not globally unique.
pub(crate) fn artifact_id_for_uri(uri: &str) -> String {
    use std::{
        collections::hash_map::DefaultHasher,
        hash::{Hash, Hasher},
    };
    let mut h = DefaultHasher::new();
    uri.hash(&mut h);
    format!("res-{:016x}", h.finish())
}

/// Translate an image [`ContentPart`] into an artifact.
///
/// The `data` field is treated as either a base64 payload (the
/// provider-native convention) or a URL — distinguished by a
/// quick prefix sniff so an MCP image resource that ships a
/// URL stays a `Part::Url` rather than being misrouted to
/// `Part::Raw`.
pub(crate) fn image_to_artifact(img: &ImageContent) -> Artifact {
    let content = if looks_like_url(&img.data) {
        PartContent::Url(img.data.clone())
    } else {
        // Provider image data is conventionally base64; pass
        // it through as `Part::Raw` and let downstream
        // consumers decode it.
        PartContent::Raw(img.data.as_bytes().to_vec())
    };
    let part = Part {
        content,
        filename: None,
        media_type: Some(img.mime_type.clone()),
        metadata: None,
    };
    Artifact {
        artifact_id: format!(
            "img-{}",
            artifact_id_for_uri(&format!("{}:{}", img.mime_type, img.data))
        ),
        name: None,
        description: None,
        parts: vec![part],
        metadata: None,
        extensions: None,
    }
}

/// Translate an audio [`ContentPart`] into an artifact.
pub(crate) fn audio_to_artifact(audio: &AudioContent) -> Artifact {
    let content = if looks_like_url(&audio.data) {
        PartContent::Url(audio.data.clone())
    } else {
        PartContent::Raw(audio.data.as_bytes().to_vec())
    };
    let part = Part {
        content,
        filename: None,
        media_type: Some(audio.mime_type.clone()),
        metadata: None,
    };
    Artifact {
        artifact_id: format!(
            "aud-{}",
            artifact_id_for_uri(&format!("{}:{}", audio.mime_type, audio.data))
        ),
        name: None,
        description: None,
        parts: vec![part],
        metadata: None,
        extensions: None,
    }
}

fn looks_like_url(s: &str) -> bool {
    s.starts_with("http://")
        || s.starts_with("https://")
        || s.starts_with("file://")
}

/// Translate a [`ToolUse`] into its wire `Part::data` payload.
///
/// The payload is the natural provider JSON shape
/// `{ id, name, input }` — the same shape
/// [`super::task_history::TaskHistoryBuilder`] writes to
/// `Task.history`, and the same shape the frontend's
/// `classifyPartPayload` / `extractFromMessage` detect as a
/// tool_call. There is no synthetic `kind` discriminator.
fn tool_use_to_data_value(tu: &ToolUse) -> serde_json::Value {
    serde_json::json!({
        "id": tu.id,
        "name": tu.name,
        "input": tu.input,
    })
}

/// Translate a [`ToolResult`] into its wire `Part::data` payload.
///
/// The payload is the natural provider JSON shape
/// `{ tool_use_id, content, is_error }` — matches the
/// front-end classifier and the `Task.history` writer.
///
/// `content` is a joined text preview of the result's `Text`
/// parts, but we also surface any embedded `ResourceLink`s as
/// a `resources` array so the frontend (single-channel
/// `task.history` reader) can render them as clickable
/// attachments without needing the separate artifact channel.
/// The same `ResourceLink` is also emitted on the artifact
/// channel (see [`super::task_history::TaskHistoryBuilder`])
/// so `task.artifacts` carries the authoritative copy.
fn tool_result_to_data_value(tr: &ToolResult) -> serde_json::Value {
    let preview_text: String = tr
        .content
        .iter()
        .filter_map(|p| match p {
            ContentPart::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect();

    let resources: Vec<serde_json::Value> = tr
        .content
        .iter()
        .filter_map(|p| match p {
            ContentPart::Resource(link) => Some(serde_json::json!({
                "uri": link.uri,
                "name": link.name,
                "mime_type": link.mime_type,
            })),
            _ => None,
        })
        .collect();

    let mut payload = serde_json::json!({
        "tool_use_id": tr.tool_use_id,
        "content": preview_text,
        "is_error": tr.is_error.unwrap_or(false),
    });

    if !resources.is_empty() {
        payload["resources"] = serde_json::Value::Array(resources);
    }

    payload
}

/// Extract the first text part from an A2A `Message`.
pub fn extract_text_from_message(msg: &Message) -> Option<String> {
    msg.parts.iter().find_map(|p| {
        if let PartContent::Text(t) = &p.content {
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
            state: normalize_task_state(state),
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
#[cfg(test)]
fn first_data_value(msg: &Message) -> Option<serde_json::Value> {
    let part = msg.parts.first()?;
    if let PartContent::Data(v) = &part.content {
        Some(v.clone())
    } else {
        None
    }
}

/// Extract the `Part::Url` payload + filename + media_type from
/// a single-part artifact on a `TaskArtifactUpdateEvent`.
#[cfg(test)]
fn first_url_value(
    artifact: &Artifact,
) -> Option<(String, Option<String>, Option<String>)> {
    let part = artifact.parts.first()?;
    if let PartContent::Url(u) = &part.content {
        Some((u.clone(), part.filename.clone(), part.media_type.clone()))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use synthia_agent::events::{SessionEndReason, SystemEvent};
    use synthia_provider::{
        ContentPart,
        ResourceLink,
        TextContent,
        ToolUse,
        types::{AudioContent, AudioFormat, ImageContent},
    };

    use super::*;

    fn test_ids() -> (TaskId, String) {
        ("task-1".to_string(), "ctx-1".to_string())
    }

    #[test]
    fn session_started_maps_to_working() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::System(SystemEvent::SessionStarted {
            session_id: "s1".to_string(),
        });
        let results = agent_event_to_stream_responses(&event, 0, &tid, &cid);
        assert_eq!(results.len(), 1);
        match results[0].as_ref().unwrap() {
            StreamResponse::StatusUpdate(su) => {
                assert_eq!(su.status.state, TaskState::Working);
            }
            other => panic!("expected StatusUpdate, got {other:?}"),
        }
    }

    #[test]
    fn message_maps_to_text_part() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::Model(ContentPart::Text(TextContent {
            text: "hi".to_string(),
            cache_control: None,
        }));
        let results = agent_event_to_stream_responses(&event, 0, &tid, &cid);
        match results[0].as_ref().unwrap() {
            StreamResponse::Message(msg) => {
                assert_eq!(msg.role, Role::Agent);
                assert_eq!(msg.parts.len(), 1);
                if let PartContent::Text(t) = &msg.parts[0].content {
                    assert_eq!(t, "hi");
                } else {
                    panic!("expected Text part");
                }
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[test]
    fn tool_call_emits_message_only() {
        // A2A v1.0 §3.7: tool calls are communication turns, not
        // tangible deliverables. The MVP routes them through the
        // `Message` channel as `Part::data` carrying the natural
        // provider `{ id, name, input }` shape — no synthetic
        // `kind` discriminator and no parallel `ArtifactUpdate`.
        let (tid, cid) = test_ids();
        let event = AgentEvent::Model(ContentPart::ToolUse(ToolUse {
            id: "u1".to_string(),
            name: "read_file".to_string(),
            input: serde_json::json!({"path": "/tmp/test"}),
        }));
        let results = agent_event_to_stream_responses(&event, 0, &tid, &cid);
        assert_eq!(results.len(), 1, "Message(Part::data) only");

        match results[0].as_ref().unwrap() {
            StreamResponse::Message(msg) => {
                let data = first_data_value(msg).expect("data part");
                // Natural provider shape — no `kind` discriminator.
                assert_eq!(data["id"], "u1");
                assert_eq!(data["name"], "read_file");
                assert_eq!(data["input"]["path"], "/tmp/test");
                assert!(
                    data.get("kind").is_none(),
                    "wire shape must not carry a synthetic `kind` key"
                );
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[test]
    fn tool_result_emits_message_only() {
        // A2A v1.0 §3.7: tool results ride the same single
        // channel as tool calls (`Message(Part::data)`); no
        // `ArtifactUpdate` is emitted.
        let (tid, cid) = test_ids();
        let event = AgentEvent::Model(ContentPart::ToolResult(ToolResult {
            tool_use_id: "u1".to_string(),
            tool_name: None,
            content: vec![ContentPart::Text(TextContent {
                text: "file contents".to_string(),
                cache_control: None,
            })],
            structured_content: None,
            is_error: Some(false),
            metadata: serde_json::Map::new(),
            truncated_by: None,
        }));
        let results = agent_event_to_stream_responses(&event, 0, &tid, &cid);
        assert_eq!(results.len(), 1, "Message(Part::data) only");

        match results[0].as_ref().unwrap() {
            StreamResponse::Message(msg) => {
                let data = first_data_value(msg).expect("data part");
                assert_eq!(data["tool_use_id"], "u1");
                assert_eq!(data["content"], "file contents");
                assert_eq!(data["is_error"], false);
                assert!(
                    data.get("resources").is_none(),
                    "resources[] MUST only appear when tool_result carries ResourceLink parts"
                );
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[test]
    fn tool_result_with_resource_link_surfaces_resources_array() {
        // When a `ToolResult.content` embeds `ResourceLink`s
        // (e.g. a file-fetch tool returning `ContentPart::Resource`),
        // the wire payload carries a `resources` array so the
        // single-channel history reader can render the
        // attachments inline. The full `ResourceLink` also rides
        // the artifact channel (see
        // `resource_link_emits_artifact_update`).
        let (tid, cid) = test_ids();
        let link = ResourceLink {
            uri: "file:///tmp/out.txt".to_string(),
            name: "out.txt".to_string(),
            title: Some("output".to_string()),
            description: Some("tool output".to_string()),
            mime_type: Some("text/plain".to_string()),
        };
        let event = AgentEvent::Model(ContentPart::ToolResult(ToolResult {
            tool_use_id: "u2".to_string(),
            tool_name: None,
            content: vec![
                ContentPart::Text(TextContent {
                    text: "fetched file".to_string(),
                    cache_control: None,
                }),
                ContentPart::Resource(link),
            ],
            structured_content: None,
            is_error: Some(false),
            metadata: serde_json::Map::new(),
            truncated_by: None,
        }));
        let results = agent_event_to_stream_responses(&event, 0, &tid, &cid);
        assert_eq!(results.len(), 1);
        match results[0].as_ref().unwrap() {
            StreamResponse::Message(msg) => {
                let data = first_data_value(msg).expect("data part");
                assert_eq!(data["tool_use_id"], "u2");
                assert_eq!(data["content"], "fetched file");
                let resources = data["resources"]
                    .as_array()
                    .expect("resources must be an array");
                assert_eq!(resources.len(), 1);
                assert_eq!(resources[0]["uri"], "file:///tmp/out.txt");
                assert_eq!(resources[0]["name"], "out.txt");
                assert_eq!(resources[0]["mime_type"], "text/plain");
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[test]
    fn resource_link_emits_artifact_update() {
        // A2A v1.0 §3.7: `ResourceLink` is a tangible deliverable,
        // not a chat turn. It rides `StreamResponse::ArtifactUpdate`
        // and lands in `Task.artifacts`. The wire part is
        // `Part::Url(uri)` (the SDK's URL-typed part — there is
        // no separate FilePart variant) with `filename` +
        // `media_type` carrying the ResourceLink's name +
        // mime_type.
        let (tid, cid) = test_ids();
        let link = ResourceLink {
            uri: "https://example.com/chart.png".to_string(),
            name: "chart.png".to_string(),
            title: Some("Q3 chart".to_string()),
            description: Some("quarterly revenue".to_string()),
            mime_type: Some("image/png".to_string()),
        };
        let event = AgentEvent::Model(ContentPart::Resource(link));
        let results = agent_event_to_stream_responses(&event, 0, &tid, &cid);
        assert_eq!(results.len(), 1, "ArtifactUpdate only");

        match results[0].as_ref().unwrap() {
            StreamResponse::ArtifactUpdate(au) => {
                // append=false / last_chunk=true for MVP single-shot emit.
                assert_eq!(au.append, Some(false));
                assert_eq!(au.last_chunk, Some(true));
                assert_eq!(
                    au.artifact.artifact_id,
                    artifact_id_for_uri("https://example.com/chart.png")
                );
                assert_eq!(au.artifact.name.as_deref(), Some("chart.png"));
                assert_eq!(
                    au.artifact.description.as_deref(),
                    Some("quarterly revenue")
                );
                let (uri, filename, media_type) =
                    first_url_value(&au.artifact).expect("Url part");
                assert_eq!(uri, "https://example.com/chart.png");
                assert_eq!(filename.as_deref(), Some("chart.png"));
                assert_eq!(media_type.as_deref(), Some("image/png"));
                let metadata = &au.artifact.parts[0].metadata;
                let metadata = metadata.as_ref().expect("metadata");
                assert_eq!(metadata.get("title").unwrap(), "Q3 chart");
                assert_eq!(metadata.get("kind").unwrap(), "resource_link");
            }
            other => panic!("expected ArtifactUpdate, got {other:?}"),
        }
    }

    #[test]
    fn resource_link_with_no_optional_fields_still_emits_artifact_update() {
        // The MVP path must not reject ResourceLinks with only
        // uri + name populated (title / description / mime_type
        // are all optional). The metadata map still carries the
        // discriminator `kind = "resource_link"` so the frontend
        // can dispatch on artifact type without sniffing the
        // Part variant — that's intentional, not "left over"
        // data.
        let (tid, cid) = test_ids();
        let link = ResourceLink {
            uri: "file:///x.txt".to_string(),
            name: "x.txt".to_string(),
            title: None,
            description: None,
            mime_type: None,
        };
        let event = AgentEvent::Model(ContentPart::Resource(link));
        let results = agent_event_to_stream_responses(&event, 0, &tid, &cid);
        match results[0].as_ref().unwrap() {
            StreamResponse::ArtifactUpdate(au) => {
                assert_eq!(au.artifact.parts.len(), 1);
                let part = &au.artifact.parts[0];
                assert!(
                    matches!(&part.content, PartContent::Url(u) if u == "file:///x.txt")
                );
                assert_eq!(part.filename.as_deref(), Some("x.txt"));
                // media_type is None when absent.
                assert!(part.media_type.is_none());
                let metadata =
                    part.metadata.as_ref().expect("kind discriminator");
                assert_eq!(metadata.get("kind").unwrap(), "resource_link");
                assert!(metadata.get("title").is_none());
                assert!(metadata.get("description").is_none());
            }
            other => panic!("expected ArtifactUpdate, got {other:?}"),
        }
    }

    #[test]
    fn image_emits_artifact_update_with_url_part_for_url_data() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::Model(ContentPart::Image(ImageContent {
            data: "https://example.com/cat.png".to_string(),
            mime_type: "image/png".to_string(),
            detail: None,
        }));
        let results = agent_event_to_stream_responses(&event, 0, &tid, &cid);
        match results[0].as_ref().unwrap() {
            StreamResponse::ArtifactUpdate(au) => {
                let part = &au.artifact.parts[0];
                assert!(
                    matches!(&part.content, PartContent::Url(u) if u == "https://example.com/cat.png")
                );
                assert_eq!(part.media_type.as_deref(), Some("image/png"));
                assert_eq!(au.last_chunk, Some(true));
            }
            other => panic!("expected ArtifactUpdate, got {other:?}"),
        }
    }

    #[test]
    fn image_emits_artifact_update_with_raw_part_for_base64_data() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::Model(ContentPart::Image(ImageContent {
            data: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=".to_string(),
            mime_type: "image/png".to_string(),
            detail: None,
        }));
        let results = agent_event_to_stream_responses(&event, 0, &tid, &cid);
        match results[0].as_ref().unwrap() {
            StreamResponse::ArtifactUpdate(au) => {
                let part = &au.artifact.parts[0];
                assert!(matches!(&part.content, PartContent::Raw(_)));
                assert_eq!(part.media_type.as_deref(), Some("image/png"));
            }
            other => panic!("expected ArtifactUpdate, got {other:?}"),
        }
    }

    #[test]
    fn audio_emits_artifact_update_with_correct_media_type() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::Model(ContentPart::Audio(AudioContent {
            data: "https://example.com/clip.mp3".to_string(),
            mime_type: "audio/mpeg".to_string(),
            format: Some(AudioFormat::Mp3),
        }));
        let results = agent_event_to_stream_responses(&event, 0, &tid, &cid);
        match results[0].as_ref().unwrap() {
            StreamResponse::ArtifactUpdate(au) => {
                let part = &au.artifact.parts[0];
                assert!(matches!(&part.content, PartContent::Url(_)));
                assert_eq!(part.media_type.as_deref(), Some("audio/mpeg"));
            }
            other => panic!("expected ArtifactUpdate, got {other:?}"),
        }
    }

    #[test]
    fn looks_like_url_recognises_url_schemes() {
        assert!(looks_like_url("https://example.com"));
        assert!(looks_like_url("http://x"));
        assert!(looks_like_url("file:///tmp/x"));
        assert!(!looks_like_url("plain text"));
        assert!(!looks_like_url("iVBORw0KGgo="));
    }

    #[test]
    fn artifact_id_for_uri_is_deterministic_and_prefixed() {
        let a1 = artifact_id_for_uri("file:///x.txt");
        let a2 = artifact_id_for_uri("file:///x.txt");
        assert_eq!(a1, a2);
        assert!(a1.starts_with("res-"));
        let b = artifact_id_for_uri("file:///y.txt");
        assert_ne!(a1, b);
    }

    #[test]
    fn reasoning_is_dropped() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::Model(ContentPart::Reasoning(
            synthia_provider::ReasoningContent {
                text: "thinking…".to_string(),
                signature: None,
            },
        ));
        let results = agent_event_to_stream_responses(&event, 0, &tid, &cid);
        assert!(results.is_empty(), "Reasoning must be dropped");
    }

    #[test]
    fn session_ended_clean_maps_to_completed() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::System(SystemEvent::SessionEnded {
            reason: SessionEndReason::Completed,
        });
        let results = agent_event_to_stream_responses(&event, 0, &tid, &cid);
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
        let results = agent_event_to_stream_responses(&event, 0, &tid, &cid);
        match results[0].as_ref().unwrap() {
            StreamResponse::StatusUpdate(su) => {
                assert_eq!(su.status.state, TaskState::Canceled);
            }
            other => panic!("expected StatusUpdate, got {other:?}"),
        }
    }

    #[test]
    fn heartbeat_tool_progress_maps_to_working() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::System(SystemEvent::ToolProgress {
            tool_name: "heartbeat".to_string(),
            call_id: "heartbeat".to_string(),
            output: synthia_tool::ToolOutput::text(String::new()),
        });
        let results = agent_event_to_stream_responses(&event, 0, &tid, &cid);
        assert_eq!(results.len(), 1);
        match results[0].as_ref().unwrap() {
            StreamResponse::StatusUpdate(su) => {
                assert_eq!(su.status.state, TaskState::Working);
                assert!(
                    su.metadata.is_none(),
                    "heartbeat MUST NOT carry metadata (no consumer)"
                );
            }
            other => panic!("expected StatusUpdate, got {other:?}"),
        }
    }

    #[test]
    fn non_heartbeat_tool_progress_is_dropped() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::System(SystemEvent::ToolProgress {
            tool_name: "shell".to_string(),
            call_id: "call-1".to_string(),
            output: synthia_tool::ToolOutput::text("running".to_string()),
        });
        let results = agent_event_to_stream_responses(&event, 0, &tid, &cid);
        assert!(
            results.is_empty(),
            "non-heartbeat ToolProgress should be dropped (MVP scope), got {}",
            results.len()
        );
    }

    #[test]
    fn session_ended_error_maps_to_failed() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::System(SystemEvent::SessionEnded {
            reason: SessionEndReason::Error("oops".to_string()),
        });
        let results = agent_event_to_stream_responses(&event, 0, &tid, &cid);
        match results[0].as_ref().unwrap() {
            StreamResponse::StatusUpdate(su) => {
                assert_eq!(su.status.state, TaskState::Failed);
            }
            other => panic!("expected StatusUpdate, got {other:?}"),
        }
    }

    #[test]
    fn session_ended_max_iterations_maps_to_failed() {
        let (tid, cid) = test_ids();
        let event = AgentEvent::System(SystemEvent::SessionEnded {
            reason: SessionEndReason::MaxIterations,
        });
        let results = agent_event_to_stream_responses(&event, 0, &tid, &cid);
        match results[0].as_ref().unwrap() {
            StreamResponse::StatusUpdate(su) => {
                assert_eq!(su.status.state, TaskState::Failed);
            }
            other => panic!("expected StatusUpdate, got {other:?}"),
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

    #[test]
    fn normalize_task_state_passes_through_canonical_variants() {
        for state in [
            TaskState::Submitted,
            TaskState::Working,
            TaskState::Completed,
            TaskState::Failed,
            TaskState::Canceled,
            TaskState::InputRequired,
            TaskState::Rejected,
            TaskState::AuthRequired,
        ] {
            assert_eq!(normalize_task_state(state.clone()), state);
        }
    }

    #[test]
    fn normalize_task_state_downgrades_unspecified_to_failed() {
        assert_eq!(
            normalize_task_state(TaskState::Unspecified),
            TaskState::Failed
        );
    }

    #[test]
    fn agent_wrapper_delegated_traces_are_dropped_at_a2a_boundary() {
        let (tid, cid) = test_ids();
        let inner = AgentEvent::Model(ContentPart::Text(TextContent {
            text: "judge picked proposal #2".into(),
            cache_control: None,
        }));
        let event = AgentEvent::Agent(
            synthia_agent::events::AgentMeta::new("root", "judge", 1),
            Box::new(inner),
        );
        let results = agent_event_to_stream_responses(&event, 0, &tid, &cid);
        assert!(
            results.is_empty(),
            "Agent wrapper must be dropped at A2A boundary, got {} response(s)",
            results.len()
        );
    }

    #[test]
    fn model_done_is_dropped_at_a2a_boundary() {
        let (tid, cid) = test_ids();
        let event =
            AgentEvent::ModelDone(synthia_provider::SamplingResult::default());
        let results = agent_event_to_stream_responses(&event, 0, &tid, &cid);
        assert!(
            results.is_empty(),
            "ModelDone must be dropped at A2A boundary, got {} response(s)",
            results.len()
        );
    }

    #[test]
    fn session_started_emits_exactly_one_status_update() {
        let (tid, cid) = test_ids();
        let event =
            AgentEvent::System(synthia_agent::SystemEvent::SessionStarted {
                session_id: "s1".into(),
            });
        let results = agent_event_to_stream_responses(&event, 0, &tid, &cid);
        assert_eq!(results.len(), 1, "expected 1 response, got {results:?}");
        match &results[0] {
            Ok(StreamResponse::StatusUpdate(su)) => {
                assert!(matches!(su.status.state, TaskState::Working));
            }
            other => panic!("expected StatusUpdate(Working), got {other:?}"),
        }
    }
}
