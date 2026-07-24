//! Replay harness for agent session events.
//!
//! Reads the durable JSONL event log written by the agent loop and
//! reconstructs a `LoopContext`-equivalent state plus the list of
//! `TurnTask`s that executed during the session.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use futures::Stream;
use synthia_session::store::PersistedEvent;
use synthia_telemetry::SpanContext;

use crate::{
    events::{
        SAMPLE_COMPLETED,
        SESSION_ENDED,
        SessionEndReason,
        TOOL_CALL_ISSUED,
        TOOL_RESULT_RECEIVED,
        TURN_COMPLETED,
        TURN_FAILED,
        TURN_STARTED,
    },
    loop_context::LoopContext,
    turn::{TurnId, TurnStatus, TurnTask},
};

const EVENTS_FILE: &str = "events.jsonl";

/// Errors that can occur while replaying a session event log.
#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    /// A line in `events.jsonl` could not be deserialized.
    #[error("corrupted event line {line_number}: {source}")]
    CorruptedLine {
        line_number: usize,
        #[source]
        source: serde_json::Error,
    },
    /// An underlying I/O error occurred.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// State reconstructed from replaying a session event log.
pub struct ReplayProjection {
    /// Loop state at the end of the replayed log.
    pub loop_state: LoopContext,
    /// Reconstructed turns, ordered by first appearance.
    pub turns: Vec<TurnTask>,
    /// 1-based line numbers that failed to deserialize.
    pub corrupted_lines: Vec<usize>,
}

impl ReplayProjection {
    /// Create a fresh projection for `session_id`.
    fn new(session_id: impl Into<String>) -> Self {
        let session_id = session_id.into();
        Self {
            loop_state: LoopContext::new(
                session_id.clone(),
                SpanContext::new(&session_id),
            ),
            turns: Vec::new(),
            corrupted_lines: Vec::new(),
        }
    }
}

/// Return a stream of deserialized events from `{session_path}/events.jsonl`.
///
/// Corrupted lines are yielded as `Err(ReplayError::CorruptedLine)` so the
/// caller can decide whether to abort or continue. Empty lines are skipped.
pub fn replay_event_stream(
    session_path: impl AsRef<Path>,
) -> impl Stream<Item = std::result::Result<PersistedEvent, ReplayError>> {
    let path = session_path.as_ref().to_path_buf();
    async_stream::try_stream! {
        let file_path = path.join(EVENTS_FILE);
        if !file_path.exists() {
            return;
        }
        let file = tokio::fs::File::open(&file_path).await?;
        let reader = tokio::io::BufReader::new(file);
        let mut lines = tokio::io::AsyncBufReadExt::lines(reader);
        let mut line_number = 0usize;
        while let Some(line) = lines.next_line().await? {
            line_number += 1;
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<PersistedEvent>(&line) {
                Ok(event) => yield event,
                Err(e) => yield Err(ReplayError::CorruptedLine { line_number, source: e })?,
            }
        }
    }
}

/// Replay all events from `{session_path}/events.jsonl` and project the
/// resulting loop state and turn list.
///
/// Corrupted lines are recorded in [`ReplayProjection::corrupted_lines`]
/// but do not abort replay.
pub fn replay_session(
    session_path: impl AsRef<Path>,
) -> Result<ReplayProjection> {
    let session_path = session_path.as_ref();
    let session_id = infer_session_id(session_path);
    let mut projection = ReplayProjection::new(session_id);

    let events = read_events_skip_corrupted(
        session_path,
        &mut projection.corrupted_lines,
    )
    .with_context(|| {
        format!("Failed to read events from {:?}", session_path)
    })?;

    for event in events {
        apply_event(&mut projection, &event);
    }

    Ok(projection)
}

/// Build the conventional session path from a user-scoped root and session id.
pub fn session_path(
    root: impl AsRef<Path>,
    user_id: &str,
    session_id: &str,
) -> PathBuf {
    root.as_ref().join(user_id).join(session_id)
}

fn read_events_skip_corrupted(
    session_path: &Path,
    corrupted_lines: &mut Vec<usize>,
) -> Result<Vec<PersistedEvent>> {
    let file_path = session_path.join(EVENTS_FILE);
    if !file_path.exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(&file_path).with_context(|| {
        format!("Failed to read events file: {:?}", file_path)
    })?;

    let mut events = Vec::new();
    for (line_number, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<PersistedEvent>(line) {
            Ok(event) => events.push(event),
            Err(_) => corrupted_lines.push(line_number + 1),
        }
    }
    Ok(events)
}

fn infer_session_id(session_path: &Path) -> String {
    session_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string()
}

fn apply_event(projection: &mut ReplayProjection, event: &PersistedEvent) {
    if event.ephemeral {
        return;
    }
    match event.event_type.as_str() {
        TURN_STARTED => {
            if let Some(turn_id) = parse_turn_id(&event.payload) {
                projection.loop_state.current_turn_id = Some(turn_id);
                projection.turns.push(TurnTask::new(
                    projection.loop_state.session_id.clone(),
                ));
                if let Some(t) = projection.turns.last_mut() {
                    t.id = turn_id;
                }
            }
            update_iteration(&mut projection.loop_state, &event.payload);
        }
        SAMPLE_COMPLETED | TOOL_CALL_ISSUED | TOOL_RESULT_RECEIVED => {
            if let Some(turn_id) = parse_turn_id(&event.payload) {
                update_turn_status(
                    &mut projection.turns,
                    turn_id,
                    TurnStatus::Executing,
                );
            }
            update_iteration(&mut projection.loop_state, &event.payload);
        }
        TURN_COMPLETED => {
            if let Some(turn_id) = parse_turn_id(&event.payload) {
                update_turn_status(
                    &mut projection.turns,
                    turn_id,
                    TurnStatus::Completed,
                );
            }
            update_iteration(&mut projection.loop_state, &event.payload);
        }
        TURN_FAILED => {
            if let Some(turn_id) = parse_turn_id(&event.payload) {
                let reason = event.payload["data"]["reason"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string();
                fail_turn(&mut projection.turns, turn_id, reason);
            }
            update_iteration(&mut projection.loop_state, &event.payload);
        }
        SESSION_ENDED => {
            if let Some(reason) = parse_session_end_reason(&event.payload) {
                projection.loop_state.end_reason = Some(reason);
            }
            update_iteration(&mut projection.loop_state, &event.payload);
        }
        _ => {}
    }
}

fn update_iteration(ctx: &mut LoopContext, payload: &serde_json::Value) {
    if let Some(iteration) = payload["iteration"].as_u64() {
        ctx.iteration = ctx.iteration.max(iteration as usize);
    }
}

fn parse_turn_id(payload: &serde_json::Value) -> Option<TurnId> {
    serde_json::from_value(payload["turn_id"].clone()).ok()
}

fn parse_session_end_reason(
    payload: &serde_json::Value,
) -> Option<SessionEndReason> {
    serde_json::from_value(payload["data"]["reason"].clone()).ok()
}

fn update_turn_status(
    turns: &mut [TurnTask],
    turn_id: TurnId,
    status: TurnStatus,
) {
    if let Some(turn) = turns.iter_mut().find(|t| t.id == turn_id) {
        turn.transition_to(status);
    }
}

fn fail_turn(turns: &mut [TurnTask], turn_id: TurnId, reason: String) {
    if let Some(turn) = turns.iter_mut().find(|t| t.id == turn_id) {
        turn.fail_with(reason);
    }
}

/// Reconstruct a list of turns from a slice of persisted events.
///
/// This is the pure, deterministic half of replay: it does not touch disk
/// and produces the same output for the same input every time.
pub fn reconstruct_turns(events: &[PersistedEvent]) -> Vec<TurnTask> {
    let mut turns: Vec<TurnTask> = Vec::new();
    for event in events {
        apply_turn_event(&mut turns, event);
    }
    turns
}

fn apply_turn_event(turns: &mut Vec<TurnTask>, event: &PersistedEvent) {
    if event.ephemeral {
        return;
    }
    match event.event_type.as_str() {
        TURN_STARTED => {
            if let Some(turn_id) = parse_turn_id(&event.payload) {
                let mut turn = TurnTask::new("");
                turn.id = turn_id;
                turns.push(turn);
            }
        }
        TURN_COMPLETED => {
            if let Some(turn_id) = parse_turn_id(&event.payload) {
                update_turn_status(turns, turn_id, TurnStatus::Completed);
            }
        }
        TURN_FAILED => {
            if let Some(turn_id) = parse_turn_id(&event.payload) {
                let reason = event.payload["data"]["reason"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string();
                fail_turn(turns, turn_id, reason);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, io::Write};

    use futures::StreamExt;
    use synthia_session::store::{EventSource, EventStore};
    use tempfile::TempDir;

    use super::*;
    use crate::events::append_agent_event;

    fn temp_session_path() -> (TempDir, PathBuf) {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("user-1").join("session-1");
        (temp, path)
    }

    #[tokio::test]
    async fn replay_event_stream_yields_events() {
        let (_temp, path) = temp_session_path();
        let turn_id = TurnId::new();
        append_agent_event(
            &EventStore::new(),
            &path,
            "session-1",
            TURN_STARTED,
            turn_id,
            1,
            (),
        )
        .await
        .unwrap();

        let events: Vec<_> = replay_event_stream(&path).collect().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].as_ref().unwrap().event_type, TURN_STARTED);
    }

    #[tokio::test]
    async fn replay_session_empty_log() {
        let (_temp, path) = temp_session_path();
        let projection = replay_session(&path).unwrap();
        assert_eq!(projection.loop_state.iteration, 0);
        assert!(projection.loop_state.end_reason.is_none());
        assert!(projection.turns.is_empty());
        assert!(projection.corrupted_lines.is_empty());
    }

    #[tokio::test]
    async fn replay_session_single_turn() {
        let (_temp, path) = temp_session_path();
        let turn_id = TurnId::new();
        append_agent_event(
            &EventStore::new(),
            &path,
            "session-1",
            TURN_STARTED,
            turn_id,
            1,
            (),
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
        append_agent_event(
            &EventStore::new(),
            &path,
            "session-1",
            TURN_COMPLETED,
            turn_id,
            1,
            (),
        )
        .await
        .unwrap();
        append_agent_event(
            &EventStore::new(),
            &path,
            "session-1",
            SESSION_ENDED,
            turn_id,
            1,
            serde_json::json!({"reason": "Completed"}),
        )
        .await
        .unwrap();

        let projection = replay_session(&path).unwrap();
        assert_eq!(projection.loop_state.iteration, 1);
        assert_eq!(
            projection.loop_state.end_reason,
            Some(SessionEndReason::Completed)
        );
        assert_eq!(projection.turns.len(), 1);
        assert_eq!(projection.turns[0].id, turn_id);
        assert_eq!(projection.turns[0].status, TurnStatus::Completed);
        assert!(projection.corrupted_lines.is_empty());
    }

    #[tokio::test]
    async fn replay_session_skips_corrupted_line() {
        let (_temp, path) = temp_session_path();
        let turn_id = TurnId::new();
        append_agent_event(
            &EventStore::new(),
            &path,
            "session-1",
            TURN_STARTED,
            turn_id,
            1,
            (),
        )
        .await
        .unwrap();
        append_agent_event(
            &EventStore::new(),
            &path,
            "session-1",
            TURN_COMPLETED,
            turn_id,
            1,
            (),
        )
        .await
        .unwrap();

        // Append a corrupted line manually.
        fs::OpenOptions::new()
            .append(true)
            .open(path.join(EVENTS_FILE))
            .unwrap()
            .write_all(b"not-json\n")
            .unwrap();

        let projection = replay_session(&path).unwrap();
        assert_eq!(projection.turns.len(), 1);
        assert_eq!(projection.turns[0].status, TurnStatus::Completed);
        assert_eq!(projection.corrupted_lines, vec![3]);
    }

    #[tokio::test]
    async fn replay_session_is_idempotent() {
        let (_temp, path) = temp_session_path();
        let turn_id = TurnId::new();
        append_agent_event(
            &EventStore::new(),
            &path,
            "session-1",
            TURN_STARTED,
            turn_id,
            1,
            (),
        )
        .await
        .unwrap();
        append_agent_event(
            &EventStore::new(),
            &path,
            "session-1",
            TURN_COMPLETED,
            turn_id,
            1,
            (),
        )
        .await
        .unwrap();

        let first = replay_session(&path).unwrap();
        let second = replay_session(&path).unwrap();
        assert_eq!(first.loop_state.iteration, second.loop_state.iteration);
        assert_eq!(first.loop_state.end_reason, second.loop_state.end_reason);
        assert_eq!(first.turns.len(), second.turns.len());
        assert_eq!(first.turns[0].id, second.turns[0].id);
        assert_eq!(first.turns[0].status, second.turns[0].status);
        assert_eq!(first.corrupted_lines, second.corrupted_lines);
    }

    #[tokio::test]
    async fn reconstruct_turns_from_failed_turn() {
        let turn_id = TurnId::new();
        let events = vec![
            PersistedEvent {
                seq: 1,
                aggregate: "session-1".to_string(),
                event_type: TURN_STARTED.to_string(),
                ts: chrono::Utc::now(),
                source: EventSource::Agent,
                ephemeral: false,
                payload: serde_json::json!({
                    "turn_id": turn_id,
                    "iteration": 1,
                    "data": {},
                }),
            },
            PersistedEvent {
                seq: 2,
                aggregate: "session-1".to_string(),
                event_type: TURN_FAILED.to_string(),
                ts: chrono::Utc::now(),
                source: EventSource::Agent,
                ephemeral: false,
                payload: serde_json::json!({
                    "turn_id": turn_id,
                    "iteration": 1,
                    "data": {"reason": "cancelled"},
                }),
            },
        ];

        let turns = reconstruct_turns(&events);
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].id, turn_id);
        assert_eq!(turns[0].status, TurnStatus::Failed);
        assert_eq!(turns[0].error_reason, Some("cancelled".to_string()));
    }

    #[test]
    fn session_path_builds_user_scoped_directory() {
        let path = session_path("/tmp/sessions", "user-1", "session-42");
        assert_eq!(path, PathBuf::from("/tmp/sessions/user-1/session-42"));
    }

    #[tokio::test]
    async fn replay_skips_ephemeral_events() {
        let (_temp, path) = temp_session_path();
        let turn_id = TurnId::new();

        // Durable: TURN_STARTED
        append_agent_event(
            &EventStore::new(),
            &path,
            "session-1",
            TURN_STARTED,
            turn_id,
            1,
            (),
        )
        .await
        .unwrap();

        // Manually append an ephemeral event with a durable type string.
        // The ephemeral flag must take precedence over the type string.
        let ephemeral_line = serde_json::json!({
            "seq": 2,
            "aggregate": "session-1",
            "type": TURN_COMPLETED,
            "ts": chrono::Utc::now(),
            "source": "agent",
            "ephemeral": true,
            "payload": {
                "turn_id": turn_id,
                "iteration": 1,
                "data": {},
            },
        });
        fs::OpenOptions::new()
            .append(true)
            .open(path.join(EVENTS_FILE))
            .unwrap()
            .write_all(format!("{}\n", ephemeral_line).as_bytes())
            .unwrap();

        // Replay: the turn should NOT be completed (ephemeral TURN_COMPLETED skipped)
        let projection = replay_session(&path).unwrap();
        assert_eq!(projection.turns.len(), 1);
        assert_eq!(projection.turns[0].status, TurnStatus::Started);

        // Now append a real durable TURN_COMPLETED
        append_agent_event(
            &EventStore::new(),
            &path,
            "session-1",
            TURN_COMPLETED,
            turn_id,
            1,
            (),
        )
        .await
        .unwrap();

        // Replay: the turn should now be Completed
        let projection = replay_session(&path).unwrap();
        assert_eq!(projection.turns.len(), 1);
        assert_eq!(projection.turns[0].status, TurnStatus::Completed);
    }

    #[tokio::test]
    async fn replay_old_format_jsonl_without_ephemeral_field() {
        let (_temp, path) = temp_session_path();
        let turn_id = TurnId::new();

        // Write an old-format JSON line (no "ephemeral" field)
        let old_format_line = serde_json::json!({
            "seq": 1,
            "aggregate": "session-1",
            "type": TURN_STARTED,
            "ts": chrono::Utc::now(),
            "source": "agent",
            "payload": {
                "turn_id": turn_id,
                "iteration": 1,
                "data": {},
            },
        });
        fs::create_dir_all(&path).unwrap();
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path.join(EVENTS_FILE))
            .unwrap()
            .write_all(format!("{}\n", old_format_line).as_bytes())
            .unwrap();

        // Replay: old-format event should be treated as durable (ephemeral defaults to false)
        let projection = replay_session(&path).unwrap();
        assert_eq!(projection.turns.len(), 1);
        assert_eq!(projection.turns[0].id, turn_id);
    }
}
