//! Typed-event projection over a [`SessionSink`].
//!
//! The legacy `events_to_messages` projection in `session.rs`
//! handles the opaque `serde_json::Value` envelopes the agent
//! loop has emitted since v0.1 (the `{"type": "Message",
//! "data": <Message>}` envelope, plus the `{"type":
//! "UserInput"/"Model"}` shapes from the server controller).
//! This module sits on top of the typed-event layer introduced
//! in [`synthia-session::SessionEvent`]: it parses the durable
//! log into typed events, runs [`synthia_session::fold_surface`],
//! and projects the result back into `synthia_provider::Message`
//! for the conversation tier of [`Memory`].
//!
//! ## Behaviour
//!
//! - Each typed surface event becomes one [`Message`] in the
//!   order produced by `fold_surface`. `UserMessage` events map
//!   to `Role::User`, `AssistantMessage` to `Role::Assistant`,
//!   `ToolResult` to a synthetic assistant tool-result message
//!   using the `tool_use_id` field, and `Compaction` to a single
//!   assistant message carrying the summary text.
//! - Legacy envelopes (anything `SessionEvent::from_value` rejects)
//!   fall back to the existing `events_to_messages` projection
//!   so old JSONL logs keep replaying without a migration step.
//! - Replacement events are honored: when `fold_surface` produces
//!   the folded surface, edits by the `SummarizingContextManager`
//!   replace (not duplicate) earlier tool results on resume.
//!
//! [`Memory`]: crate::memory::Memory

use std::sync::Arc;

use serde_json::Value;
use synthia_provider::Message;
use synthia_session::{
    FoldedSurface,
    SessionEvent,
    SessionSink,
    try_fold_log_surface,
};
use thiserror::Error;

/// Errors the typed projection can surface.
#[derive(Debug, Error)]
pub enum TypedProjectionError {
    /// Sink I/O failure.
    #[error("sink error: {0}")]
    Sink(String),
    /// Fold failure (provenance violation or out-of-bounds
    /// replace range).
    #[error("fold error: {0}")]
    Fold(String),
}

/// Read a sink and project its events into `Vec<Message>` using
/// the typed-event layer.
///
/// Strategy:
///
/// 1. Read the full sink log.
/// 2. Parse every row into [`SessionEvent`] where the typed
///    shape matches (`SessionEvent::from_value`). Rows that
///    fail to parse (legacy `{"type": "Message"}` envelopes,
///    pre-R4 `UserInput`/`Model` shapes) fall through to
///    [`events_to_messages`](crate::memory::events_to_messages).
/// 3. Run [`fold_surface`](synthia_session::fold_surface) on the
///    typed events to get the ordered surface.
/// 4. Project each folded entry to a [`Message`].
/// 5. Concatenate the typed projection with the legacy
///    projection's output so a session mixing old and new
///    events replays correctly.
///
/// The `tail` parameter caps the projection to the last `n`
/// typed events (passed-through to the legacy path too). Pass
/// `None` for unbounded projection.
pub async fn typed_messages_from_sink(
    sink: Arc<dyn SessionSink>,
    tail: Option<usize>,
) -> Result<Vec<Message>, TypedProjectionError> {
    let events = sink
        .read()
        .await
        .map_err(|e| TypedProjectionError::Sink(e.to_string()))?;
    project(&events, tail)
}

/// Pure projection: `events` (the raw sink log) → `Vec<Message>`.
pub fn project(
    events: &[Value],
    tail: Option<usize>,
) -> Result<Vec<Message>, TypedProjectionError> {
    // One fold for the whole log — typed rows and the legacy
    // envelopes the controller wrote before the typed layer
    // existed — with every row stamped by its 1-based log ordinal
    // and compaction replacements applied. This is the same fold
    // `events_to_messages` uses; the only difference is that a
    // replacement whose provenance does not resolve is surfaced
    // here instead of being skipped.
    let folded = try_fold_log_surface(events)
        .map_err(|e| TypedProjectionError::Fold(format!("{e:?}")))?;
    Ok(surface_to_messages(&folded, tail))
}

/// Project a [`FoldedSurface`] into `Vec<Message>`.
///
/// Honours `tail`: the last `n` folded messages are returned in
/// order; earlier ones are dropped (with `tail == None`, the
/// whole surface is returned).
fn surface_to_messages(
    surface: &FoldedSurface,
    tail: Option<usize>,
) -> Vec<Message> {
    let start = match tail {
        Some(n) if surface.messages.len() > n => surface.messages.len() - n,
        _ => 0,
    };
    // Applied AFTER the tail slice, and shared with `events_to_messages`
    // (see `super::session::project_surface_messages`) so the two
    // projections cannot disagree about a turn's wire shape. A tail that
    // lands mid-turn therefore keeps whatever parts survived the slice
    // rather than reaching back past the caller's bound: `tail` exists to
    // cap the projection, so widening it to complete a turn would defeat
    // its purpose.
    super::session::project_surface_messages(&surface.messages[start..])
}

/// Repair an interrupted session by reading the sink, appending
/// synthetic closer events for any open tail turn, and writing
/// them back to the sink.
///
/// Returns the number of synthetic events appended. A zero
/// return means the log was already balanced.
pub async fn repair_session(
    sink: Arc<dyn SessionSink>,
) -> Result<usize, TypedProjectionError> {
    let events = sink
        .read()
        .await
        .map_err(|e| TypedProjectionError::Sink(e.to_string()))?;
    let typed: Vec<SessionEvent> =
        events.iter().filter_map(SessionEvent::from_value).collect();
    let closers = synthia_session::interrupted_turn_closers(&typed);
    for closer in &closers {
        let v = serde_json::to_value(closer)
            .map_err(|e| TypedProjectionError::Fold(e.to_string()))?;
        sink.append(&v)
            .await
            .map_err(|e| TypedProjectionError::Sink(e.to_string()))?;
    }
    Ok(closers.len())
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use synthia_provider::Role;
    use synthia_session::in_memory::InMemorySessionSink;

    use super::*;

    fn sink() -> Arc<InMemorySessionSink> {
        Arc::new(InMemorySessionSink::new("s1"))
    }

    #[tokio::test]
    async fn empty_sink_projects_to_empty_messages() {
        let msgs = typed_messages_from_sink(sink(), None).await.unwrap();
        assert!(msgs.is_empty());
    }

    #[tokio::test]
    async fn typed_user_message_projects_as_user_role() {
        let s = sink();
        let user = Message::user("hi");
        s.append(&json!({
            "seq": 0,
            "ts": "2026-09-10T00:00:00Z",
            "type": "user_message",
            "data": serde_json::to_value(&user).unwrap(),
            "surface_op": "append"
        }))
        .await
        .unwrap();
        let msgs = typed_messages_from_sink(s, None).await.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, Role::User);
    }

    #[tokio::test]
    async fn legacy_envelopes_fall_through_to_legacy_projection() {
        let s = sink();
        // `{"type": "UserInput", "data": {"text": "hi"}}` — the
        // pre-R4 envelope the server controller emitted.
        s.append(&json!({
            "type": "UserInput",
            "data": {"text": "hi"}
        }))
        .await
        .unwrap();
        let msgs = typed_messages_from_sink(s, None).await.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, Role::User);
    }

    #[tokio::test]
    async fn mixed_typed_and_legacy_events_compose() {
        let s = sink();
        // Legacy first (the agent loop emitted this before R4).
        s.append(&json!({
            "type": "UserInput",
            "data": {"text": "old"}
        }))
        .await
        .unwrap();
        // Typed second (the agent loop after R4). The data
        // payload is a real `Message` serde value.
        let assistant = Message::assistant("new");
        s.append(&json!({
            "seq": 99,
            "ts": "2026-09-10T00:00:00Z",
            "type": "assistant_message",
            "data": serde_json::to_value(&assistant).unwrap(),
            "surface_op": "append"
        }))
        .await
        .unwrap();
        let msgs = typed_messages_from_sink(s, None).await.unwrap();
        assert_eq!(msgs.len(), 2);
        // Order: legacy first, typed second (preserved by
        // indexed walk).
        assert_eq!(msgs[0].role, Role::User);
        assert_eq!(msgs[1].role, Role::Assistant);
    }

    #[tokio::test]
    async fn tail_cap_drops_oldest_typed_messages() {
        let s = sink();
        for i in 0..5 {
            let user = Message::user(format!("m{i}"));
            s.append(&json!({
                "seq": i,
                "ts": "2026-09-10T00:00:00Z",
                "type": "user_message",
                "data": serde_json::to_value(&user).unwrap(),
                "surface_op": "append"
            }))
            .await
            .unwrap();
        }
        let msgs = typed_messages_from_sink(s, Some(2)).await.unwrap();
        assert_eq!(msgs.len(), 2);
    }

    #[tokio::test]
    async fn repair_session_writes_closers_for_interrupted_log() {
        let s = sink();
        // Seed an interrupted log: turn/start + step/start +
        // assistant-with-tool-use + tool_call + nothing else.
        s.append(&json!({
            "seq": 1,
            "ts": "2026-09-10T00:00:00Z",
            "type": "turn",
            "data": {"kind": "start", "turn": 1}
        }))
        .await
        .unwrap();
        s.append(&json!({
            "seq": 2,
            "ts": "2026-09-10T00:00:00Z",
            "type": "step",
            "data": {"kind": "start", "turn": 1, "step": 1}
        }))
        .await
        .unwrap();
        s.append(&json!({
            "seq": 3,
            "ts": "2026-09-10T00:00:00Z",
            "type": "assistant_message",
            "data": {
                "role": "assistant",
                "content": [{"type": "text", "text": "calling"}],
                "tool_calls": [{"id": "c1", "name": "read"}]
            },
            "surface_op": "append"
        }))
        .await
        .unwrap();
        s.append(&json!({
            "seq": 4,
            "ts": "2026-09-10T00:00:00Z",
            "type": "tool_call",
            "data": {"call_id": "c1", "name": "read", "arguments": {}}
        }))
        .await
        .unwrap();
        let count = repair_session(s.clone()).await.unwrap();
        assert!(
            count >= 2,
            "expected at least tool_result + step/end + turn/end, got {count}"
        );
        // Re-reading the sink should now see the closers.
        let events = s.read().await.unwrap();
        assert!(
            events.iter().any(|v| v["type"] == "tool_result"
                && v["data"]["interrupted"] == true)
        );
        assert!(
            events
                .iter()
                .any(|v| v["type"] == "turn" && v["data"]["kind"] == "end")
        );
    }

    #[tokio::test]
    async fn repair_session_no_op_for_balanced_log() {
        let s = sink();
        s.append(&json!({
            "seq": 1,
            "ts": "2026-09-10T00:00:00Z",
            "type": "turn",
            "data": {"kind": "end", "turn": 1}
        }))
        .await
        .unwrap();
        let count = repair_session(s.clone()).await.unwrap();
        assert_eq!(count, 0);
        assert_eq!(s.len(), 1);
    }

    #[tokio::test]
    async fn fold_failure_surfaces_as_typed_projection_error() {
        let s = sink();
        // Bad replace: source_event_seqs cites a future seq.
        s.append(&json!({
            "seq": 1,
            "ts": "2026-09-10T00:00:00Z",
            "type": "user_message",
            "data": {"role": "user", "content": [{"type": "text", "text": "u1"}]},
            "surface_op": "append"
        }))
        .await
        .unwrap();
        s.append(&json!({
            "seq": 2,
            "ts": "2026-09-10T00:00:00Z",
            "type": "assistant_message",
            "data": {"role": "assistant", "content": [{"type": "text", "text": "summary"}]},
            "surface_op": {
                "start": 0,
                "end": 1,
                "source_event_seqs": [99]
            }
        }))
        .await
        .unwrap();
        let err = typed_messages_from_sink(s, None).await.unwrap_err();
        assert!(matches!(err, TypedProjectionError::Fold(_)));
    }
    /// The typed projection is PUBLIC (`typed_messages_from_sink`), so it
    /// must coalesce a turn's parallel tool calls exactly as the legacy
    /// projection does — otherwise a consumer that reads its history
    /// through this seam still sends an invalid request to OpenAI
    /// ("insufficient tool messages following tool_calls message").
    #[tokio::test]
    async fn typed_projection_merges_parallel_tool_calls() {
        let sink = sink();
        for row in [
            json!({"type": "UserInput", "data": {"text": "run both"}}),
            json!({"type": "Model", "data": {
                "type": "tool_use", "id": "call_a", "name": "shell",
                "input": {"command": "echo ONE"},
            }}),
            json!({"type": "Model", "data": {
                "type": "tool_use", "id": "call_b", "name": "shell",
                "input": {"command": "echo TWO"},
            }}),
            json!({"type": "Model", "data": {
                "type": "tool_result", "tool_use_id": "call_a",
                "tool_name": "shell", "is_error": false,
                "content": [{"type": "text", "text": "ONE"}],
            }}),
        ] {
            sink.append(&row).await.unwrap();
        }
        let events = sink.read().await.unwrap();
        let messages = project(&events, None).expect("projection");
        let assistant = messages
            .iter()
            .find(|m| m.role == Role::Assistant)
            .expect("an assistant turn");
        let ids: Vec<&str> = assistant
            .content
            .iter()
            .filter_map(|p| match p {
                synthia_provider::ContentPart::ToolUse(tu) => {
                    Some(tu.id.as_str())
                }
                _ => None,
            })
            .collect();
        assert_eq!(ids, vec!["call_a", "call_b"], "got {messages:?}");
    }
}
