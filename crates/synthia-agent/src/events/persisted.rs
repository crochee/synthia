//! Durable JSONL event persistence for agent loop transitions.
//!
//! Wraps [`synthia_session::store::events::EventStore`] and enriches every
//! payload with the current `turn_id` and `iteration` so that a session can
//! be replayed later.

use std::path::{Path, PathBuf};

use anyhow::Result;
use synthia_session::store::{EventSource, EventStore, PersistedEvent};

use crate::turn::TurnId;

/// Event type emitted when a new turn starts.
pub const TURN_STARTED: &str = "TurnStarted";
/// Event type emitted when LLM sampling completes.
pub const SAMPLE_COMPLETED: &str = "SampleCompleted";
/// Event type emitted when the LLM requests one or more tool calls.
pub const TOOL_CALL_ISSUED: &str = "ToolCallIssued";
/// Event type emitted when a tool result is received.
pub const TOOL_RESULT_RECEIVED: &str = "ToolResultReceived";
/// Event type emitted when a turn completes normally.
pub const TURN_COMPLETED: &str = "TurnCompleted";
/// Event type emitted when a turn fails.
pub const TURN_FAILED: &str = "TurnFailed";
/// Event type emitted when the agent loop ends.
pub const SESSION_ENDED: &str = "SessionEnded";

/// Returns `true` if the given persistence-layer event type string is durable.
///
/// This is the persistence-layer projection of
/// [`crate::events::AgentEvent::is_durable`]. Note the distinction:
///
/// - `AgentEvent::is_durable()` (the agent-layer method, on the
///   restructured 5-variant enum) is **exhaustive**: it inspects the
///   inner `ContentPart` for `Model(...)` events, so the durability
///   of a single `AgentEvent::Model(_)` is no longer decided by the
///   variant alone. Use this method whenever you have an actual
///   `AgentEvent` value.
/// - This function is the **string-tagged** variant used by the
///   persistence layer when persisting *legacy* turn-lifecycle event
///   tags (`TurnStarted`, `SampleCompleted`, etc.) which carry no
///   inner payload. For those tags the safe-default rule still
///   applies: any unknown tag is treated as durable.
///
/// The new top-level `AgentEvent` variants are dispatched through
/// the agent-layer method and routed to `append_agent_event` with a
/// derived type tag. Currently the type tag is the inner
/// `ContentPart` discriminant, e.g. `"ModelText"`, `"ModelReasoning"`,
/// `"ToolCallStarted"`; the set below matches those tags.
pub fn is_durable_event_type(event_type: &str) -> bool {
    // Ephemeral type tags: streaming text/reasoning deltas, progress,
    // warnings, usage, hooks. Everything else (including all unknown
    // legacy tags) defaults to durable — this is the persisted layer
    // analogue of the legacy "unknown = durable" rule, applied to the
    // string-tagged view of events.
    !matches!(
        event_type,
        // Top-level streaming Model events (legacy bare "Model" tag
        // for text-delta strings, plus per-ContentPart tags emitted
        // by the new SSE wire mapping).
        "Model"
            | "ModelText"
            | "ModelReasoning"
            | "ModelImage"
            | "ModelAudio"
            | "ModelResource"
            | "Hook"
            | "SteeringReceived"
            | "Progress"
            | "TokenBudgetNotice"
    )
}

/// Append an agent event to `{session_path}/events.jsonl`.
///
/// The payload is wrapped as `{ "data": payload, "turn_id": ..., "iteration": ... }`
/// and written with `fsync` durability. The write happens on the blocking
/// thread pool because `fsync` may block.
///
/// The caller must pass the shared [`EventStore`] from
/// [`synthia_session::Store::event_store`] so that the in-process seq cache
/// is reused across calls (O(1) after the first call per session).
#[allow(clippy::too_many_arguments)]
pub async fn append_agent_event<P>(
    store: &EventStore,
    session_path: impl AsRef<Path>,
    aggregate: impl Into<String>,
    event_type: impl Into<String>,
    turn_id: TurnId,
    iteration: usize,
    payload: P,
) -> Result<PersistedEvent>
where
    P: serde::Serialize + Send + 'static,
{
    let enriched = serde_json::json!({
        "data": payload,
        "turn_id": turn_id,
        "iteration": iteration,
    });

    let session_path = session_path.as_ref().to_path_buf();
    let aggregate = aggregate.into();
    let event_type = event_type.into();
    let ephemeral = !is_durable_event_type(&event_type);
    let store = store.clone();

    tokio::task::spawn_blocking(move || {
        store.append(
            &session_path,
            &aggregate,
            &event_type,
            EventSource::Agent,
            ephemeral,
            &enriched,
        )
    })
    .await?
}

/// Read all persisted events from `{session_path}/events.jsonl`.
pub fn read_all_events(
    session_path: impl AsRef<Path>,
) -> Result<Vec<PersistedEvent>> {
    // `EventStore::read_from` uses seq > last_seq, so passing 0 returns all
    // events and the limit is unbounded.
    let store = EventStore::new();
    store.read_from(session_path.as_ref(), 0, usize::MAX)
}

/// Build the conventional session path from a user-scoped root and session id.
pub fn session_path(
    root: impl AsRef<Path>,
    user_id: &str,
    session_id: &str,
) -> PathBuf {
    root.as_ref().join(user_id).join(session_id)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    #[tokio::test]
    async fn append_agent_event_writes_enriched_jsonl() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("session");
        let turn_id = TurnId::new();

        let event = append_agent_event(
            &EventStore::new(),
            &path,
            "session-1",
            TURN_STARTED,
            turn_id,
            7,
            serde_json::json!({ "hello": "world" }),
        )
        .await
        .unwrap();

        assert_eq!(event.seq, 1);
        assert_eq!(event.aggregate, "session-1");
        assert_eq!(event.event_type, TURN_STARTED);
        assert_eq!(event.source, EventSource::Agent);
        assert_eq!(event.payload["iteration"], 7);
        assert_eq!(
            event.payload["turn_id"],
            serde_json::to_value(turn_id).unwrap()
        );
        assert_eq!(event.payload["data"]["hello"], "world");

        let events = read_all_events(&path).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, TURN_STARTED);
    }

    #[tokio::test]
    async fn append_agent_event_is_append_only() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("session");
        let turn_id = TurnId::new();

        append_agent_event(
            &EventStore::new(),
            &path,
            "session-1",
            TURN_STARTED,
            turn_id,
            1,
            serde_json::json!({}),
        )
        .await
        .unwrap();
        append_agent_event(
            &EventStore::new(),
            &path,
            "session-1",
            SAMPLE_COMPLETED,
            turn_id,
            1,
            serde_json::json!({"text": "hi"}),
        )
        .await
        .unwrap();

        let events = read_all_events(&path).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_type, TURN_STARTED);
        assert_eq!(events[1].event_type, SAMPLE_COMPLETED);
        assert!(events[0].seq < events[1].seq);
    }

    #[tokio::test]
    async fn append_agent_event_creates_directory() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("nested").join("session");
        assert!(!path.exists());

        append_agent_event(
            &EventStore::new(),
            &path,
            "session-1",
            TURN_COMPLETED,
            TurnId::new(),
            1,
            (),
        )
        .await
        .unwrap();

        assert!(path.exists());
        assert!(path.join("events.jsonl").exists());
    }

    #[test]
    fn read_all_events_from_missing_file_returns_empty() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("empty");
        let events = read_all_events(&path).unwrap();
        assert!(events.is_empty());
    }

    #[tokio::test]
    async fn append_agent_event_fsyncs_before_returning() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("session");

        append_agent_event(
            &EventStore::new(),
            &path,
            "session-1",
            SESSION_ENDED,
            TurnId::new(),
            3,
            (),
        )
        .await
        .unwrap();

        // If the file exists and is non-empty, fsync happened.
        let metadata = fs::metadata(path.join("events.jsonl")).unwrap();
        assert!(metadata.len() > 0);
    }

    #[test]
    fn session_path_builds_user_scoped_directory() {
        let path = session_path("/tmp/sessions", "user-1", "session-42");
        assert_eq!(path, PathBuf::from("/tmp/sessions/user-1/session-42"));
    }

    #[tokio::test]
    async fn append_ephemeral_event_persists_flag() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("session");

        // "Model" is classified as ephemeral in the new world
        let event = append_agent_event(
            &EventStore::new(),
            &path,
            "session-1",
            "Model",
            TurnId::new(),
            1,
            serde_json::json!({"text": "hmm"}),
        )
        .await
        .unwrap();

        assert!(event.ephemeral);

        let events = read_all_events(&path).unwrap();
        assert_eq!(events.len(), 1);
        assert!(events[0].ephemeral);

        let raw = fs::read_to_string(path.join("events.jsonl")).unwrap();
        assert!(raw.contains("\"ephemeral\":true"));
    }
}
