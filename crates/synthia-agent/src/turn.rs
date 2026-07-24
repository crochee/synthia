//! Stable turn identifier for cross-event turn correlation in observability.
//!
//! A [`TurnId`] is a thin newtype over a `uuid::Uuid` (v4) with `Copy` and
//! `Hash` semantics, intentionally distinct from the historical
//! `"turn-{N}"` string that was derived from `LoopContext.iteration: usize`.
//! See `openspec/changes/turn-id-mvp/` for the design rationale and scope.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TurnId(pub Uuid);

impl TurnId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TurnId {
    fn default() -> Self {
        Self::new()
    }
}

/// Lifecycle status of a [`TurnTask`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TurnStatus {
    /// Turn has been created but sampling has not started.
    Started,
    /// Waiting for the LLM to produce a response.
    Sampling,
    /// Executing one or more tool calls requested by the LLM.
    Executing,
    /// Turn completed normally.
    Completed,
    /// Turn failed with a recorded reason.
    Failed,
}

/// A single turn through the agent ReAct loop.
///
/// Phase 1 models turns as sequential, inline units of work. Each turn
/// carries a stable [`TurnId`], its parent session id, and a status that
/// transitions through the lifecycle states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnTask {
    pub id: TurnId,
    pub session_id: String,
    pub status: TurnStatus,
    pub error_reason: Option<String>,
}

impl TurnTask {
    /// Create a new turn for `session_id` in the `Started` state.
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            id: TurnId::new(),
            session_id: session_id.into(),
            status: TurnStatus::Started,
            error_reason: None,
        }
    }

    /// Move the turn to a new status.
    pub fn transition_to(&mut self, status: TurnStatus) {
        self.status = status;
    }

    /// Mark the turn as failed with a reason.
    pub fn fail_with(&mut self, reason: impl Into<String>) {
        self.status = TurnStatus::Failed;
        self.error_reason = Some(reason.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turn_task_new_has_unique_id_and_started_status() {
        let turn = TurnTask::new("session-a");
        assert_eq!(turn.session_id, "session-a");
        assert_eq!(turn.status, TurnStatus::Started);
        assert!(turn.error_reason.is_none());

        let turn2 = TurnTask::new("session-a");
        assert_ne!(turn.id, turn2.id);
    }

    #[test]
    fn turn_task_transitions_through_tool_call_lifecycle() {
        let mut turn = TurnTask::new("session-b");
        turn.transition_to(TurnStatus::Sampling);
        assert_eq!(turn.status, TurnStatus::Sampling);
        turn.transition_to(TurnStatus::Executing);
        assert_eq!(turn.status, TurnStatus::Executing);
        turn.transition_to(TurnStatus::Completed);
        assert_eq!(turn.status, TurnStatus::Completed);
        assert!(turn.error_reason.is_none());
    }

    #[test]
    fn turn_task_fail_with_records_reason() {
        let mut turn = TurnTask::new("session-c");
        turn.transition_to(TurnStatus::Sampling);
        turn.fail_with("provider timeout");
        assert_eq!(turn.status, TurnStatus::Failed);
        assert_eq!(turn.error_reason, Some("provider timeout".to_string()));
    }

    #[test]
    fn turn_task_isolation_by_session() {
        let turn_a = TurnTask::new("session-a");
        let turn_b = TurnTask::new("session-b");
        assert_eq!(turn_a.session_id, "session-a");
        assert_eq!(turn_b.session_id, "session-b");
    }
}
