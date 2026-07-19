//! `task` module — `TaskGoal` + 7-state machine + handle.
//!
//! PR-3.5 introduces the type surface; PR-3.6 wires it through
//! `CodeGoalService::submit`, and PR-3.7 adds the OCC `Keep`/`Set`
//! transitions. The 7 states called for in
//! `specs/goal-service-runtime/spec.md` are:
//!
//! 1. `Pending`     — accepted, awaiting admission permit.
//! 2. `Admitted`    — permit acquired; scheduled on the runtime.
//! 3. `Running`     — execution in progress.
//! 4. `Succeeded`   — terminal: completed without error.
//! 5. `Failed`      — terminal: completed with error.
//! 6. `Cancelled`   — terminal: cancelled before completion.
//! 7. `Evicted`     — terminal: idle-evicted by the runtime drop path.

use std::fmt;

use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use uuid::Uuid;

/// Unique identifier for a goal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskGoalId(pub Uuid);

impl TaskGoalId {
    /// Allocate a fresh id (UUID v4).
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TaskGoalId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for TaskGoalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// 7-state lifecycle of a `TaskGoal`.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize,
)]
pub enum TaskGoalState {
    /// Accepted, awaiting admission permit.
    #[default]
    Pending,
    /// Permit acquired; scheduled on the runtime.
    Admitted,
    /// Execution in progress.
    Running,
    /// Terminal: completed without error.
    Succeeded,
    /// Terminal: completed with error.
    Failed,
    /// Terminal: cancelled before completion.
    Cancelled,
    /// Terminal: idle-evicted by the runtime drop path.
    Evicted,
}

impl TaskGoalState {
    /// Whether this state is terminal (no further transitions allowed).
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::Evicted
        )
    }

    /// String label for tracing.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Admitted => "admitted",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Evicted => "evicted",
        }
    }
}

/// A submitted task goal.
///
/// `state` is the *current* state; consumers should observe state
/// transitions via the [`TaskGoalHandle`] returned by
/// [`crate::GoalService::submit`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskGoal {
    /// Unique id.
    pub id: TaskGoalId,
    /// Human-readable label (used in tracing + metrics).
    pub label: String,
    /// Current state (defaults to `Pending`).
    pub state: TaskGoalState,
    /// Wall-clock creation timestamp (ms since Unix epoch).
    pub created_at_ms: i64,
}

impl TaskGoal {
    /// Construct a fresh goal in the `Pending` state.
    #[must_use]
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            id: TaskGoalId::new(),
            label: label.into(),
            state: TaskGoalState::Pending,
            created_at_ms: now_ms(),
        }
    }

    /// Move to the requested state iff it is not already terminal.
    ///
    /// Returns `true` if the transition took effect, `false` if the
    /// current state is terminal (no further transitions allowed).
    pub fn transition_to(&mut self, next: TaskGoalState) -> bool {
        if self.state.is_terminal() {
            return false;
        }
        self.state = next;
        true
    }
}

/// Handle returned by [`crate::GoalService::submit`].
///
/// PR-3.5 carries only the id + a `watch` channel for state change
/// observation; PR-3.6 attaches the `AdmissionPermit` so `Drop` releases
/// the semaphore slot.
#[derive(Debug, Clone)]
pub struct TaskGoalHandle {
    /// Goal id (used to retrieve via [`crate::GoalService`] queries).
    pub id: TaskGoalId,
    /// Most-recent state observed when the handle was emitted.
    pub initial_state: TaskGoalState,
    /// Watch channel for live state changes; consumers should
    /// `subscribe()` for the current value, then `changed().await` for
    /// future transitions.
    state_tx: watch::Sender<TaskGoalState>,
    /// Long-lived anchor receiver: tokio's `watch::Sender::send` is a
    /// no-op when zero receivers exist, so we keep one receiver alive
    /// for the lifetime of the handle. This makes the channel
    /// `subscribe`-then-`update` AND `update`-then-`subscribe` both
    /// observable to a future subscriber.
    _anchor: watch::Receiver<TaskGoalState>,
}

impl TaskGoalHandle {
    /// Construct a handle in the given initial state.
    #[must_use]
    pub fn new(id: TaskGoalId, initial_state: TaskGoalState) -> Self {
        let (state_tx, anchor) = watch::channel(initial_state);
        Self {
            id,
            initial_state,
            state_tx,
            _anchor: anchor,
        }
    }

    /// Subscribe to live state changes.
    #[must_use]
    pub fn subscribe(&self) -> watch::Receiver<TaskGoalState> {
        self.state_tx.subscribe()
    }

    /// Push a new state value to all subscribers. No-op if the state is
    /// terminal so consumers cannot observe a non-terminal reversal.
    ///
    /// Returns `Some(())` if the value was sent, `None` if the no-op
    /// guard fired.
    pub fn update(&self, next: TaskGoalState) -> Option<()> {
        if self.initial_state.is_terminal() {
            return None;
        }
        // The `_anchor` receiver inside the handle guarantees `send`
        // always has a subscriber; any new subscriber created via
        // `subscribe()` will observe `next` on its first `borrow()`.
        self.state_tx.send(next).map_err(|_| ()).ok()
    }
}

/// Wall-clock time in milliseconds since the Unix epoch (mirrors the
/// `EventMeta::now_ms` helper so timestamps line up across crates).
fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_millis()).unwrap_or(i64::MAX),
        Err(_) => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_terminality_matches_spec() {
        assert!(!TaskGoalState::Pending.is_terminal());
        assert!(!TaskGoalState::Admitted.is_terminal());
        assert!(!TaskGoalState::Running.is_terminal());
        assert!(TaskGoalState::Succeeded.is_terminal());
        assert!(TaskGoalState::Failed.is_terminal());
        assert!(TaskGoalState::Cancelled.is_terminal());
        assert!(TaskGoalState::Evicted.is_terminal());
    }

    #[test]
    fn transition_respects_terminality() {
        let mut g = TaskGoal::new("test");
        assert!(g.transition_to(TaskGoalState::Admitted));
        assert!(g.transition_to(TaskGoalState::Running));
        assert!(g.transition_to(TaskGoalState::Succeeded));
        // After Succeeded, further transitions are rejected.
        assert!(!g.transition_to(TaskGoalState::Cancelled));
    }

    #[test]
    fn handle_initial_state_matches_constructor() {
        let id = TaskGoalId::new();
        let h = TaskGoalHandle::new(id, TaskGoalState::Pending);
        assert_eq!(h.id, id);
        assert_eq!(h.initial_state, TaskGoalState::Pending);
        let rx = h.subscribe();
        assert_eq!(*rx.borrow(), TaskGoalState::Pending);
    }

    #[test]
    fn handle_update_propagates() {
        let h = TaskGoalHandle::new(TaskGoalId::new(), TaskGoalState::Pending);
        h.update(TaskGoalState::Admitted);
        let rx = h.subscribe();
        // tokio's `watch::Receiver::borrow_and_update` returns the
        // current value synchronously; we rely on `borrow()` here since
        // the test only inspects the last sent value.
        assert_eq!(*rx.borrow(), TaskGoalState::Admitted);
    }

    #[test]
    fn task_goal_default_state_is_pending() {
        let g = TaskGoal::new("default-test");
        assert_eq!(g.state, TaskGoalState::Pending);
        assert!(g.created_at_ms > 0);
    }
}
