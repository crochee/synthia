//! The stateful `SessionStateMachine` struct that owns the
//! `current_state` for one session and persists each transition via
//! the underlying `Store`.

pub use super::transitions::{
    StateEnterEffect,
    StateMachineError,
    effect_for_entering,
    is_valid_transition,
};
use crate::{
    store::Store,
    types::{InvalidStateTransition, SessionState},
};

/// `SessionStateMachine` manages session state transitions with validation,
/// persistence, and side effect signaling.
pub struct SessionStateMachine {
    current_state: SessionState,
    session_id: String,
    session_store: Store,
}

impl SessionStateMachine {
    /// Creates a new state machine for the given session.
    pub fn new(
        session_id: String,
        store: Store,
        initial: SessionState,
    ) -> Self {
        Self {
            current_state: initial,
            session_id,
            session_store: store,
        }
    }

    /// Returns the current session state.
    pub fn current_state(&self) -> SessionState {
        self.current_state
    }

    /// Returns the session ID.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Attempts to transition to the target state.
    ///
    /// Validates the transition via `is_valid_transition`, updates the session's state,
    /// persists via `session_store.save_metadata`, and returns any side effects
    /// the caller should handle.
    pub fn transition_to(
        &mut self,
        target: SessionState,
        session: &mut crate::types::Session,
    ) -> Result<StateEnterEffect, StateMachineError> {
        if !is_valid_transition(self.current_state, target) {
            return Err(StateMachineError::InvalidTransition(
                InvalidStateTransition {
                    from: self.current_state,
                    to: target,
                },
            ));
        }

        let old_state = self.current_state;
        self.current_state = target;

        // Update the session's state before persisting
        session.state = target;
        session.updated_at = chrono::Utc::now();
        session.needs_save = true;

        // Log state transitions
        tracing::info!(
            session_id = %self.session_id,
            from = ?old_state,
            to = ?target,
            "session state transition"
        );

        // Persist metadata to store
        self.session_store
            .save_metadata(session)
            .map_err(StateMachineError::Persistence)?;

        // Trigger on_state_enter side effects (logging)
        Self::on_state_enter(&self.session_id, target);

        // Return side effect hint for caller to handle
        Ok(effect_for_entering(target))
    }

    /// Executes side effects when entering a new state.
    /// This handles synchronous side effects like logging.
    /// Async side effects (e.g., timers) are signaled via `StateEnterEffect`.
    fn on_state_enter(session_id: &str, state: SessionState) {
        match state {
            SessionState::Compacting => {
                tracing::info!(
                    session_id = %session_id,
                    "starting log compaction"
                );
            }
            SessionState::WaitingForApproval => {
                tracing::info!(
                    session_id = %session_id,
                    "session waiting for approval - timeout timer should be started"
                );
            }
            SessionState::Completed => {
                tracing::info!(
                    session_id = %session_id,
                    "session completed successfully"
                );
            }
            SessionState::Error => {
                tracing::warn!(
                    session_id = %session_id,
                    "session entered error state"
                );
            }
            SessionState::Cancelled => {
                tracing::info!(
                    session_id = %session_id,
                    "session cancelled"
                );
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::{store::Store, types::Session};

    fn make_state_machine() -> (SessionStateMachine, TempDir) {
        let temp = TempDir::new().unwrap();
        let store = Store::new(temp.path().to_path_buf());
        let session_id = "test-session".to_string();
        let sm = SessionStateMachine::new(
            session_id,
            store,
            SessionState::Initializing,
        );
        (sm, temp)
    }

    fn make_session() -> Session {
        // Tests in this module exercise the state machine's persistence
        // path (transition_to -> save_metadata), which refuses to persist
        // a session with an empty user_id. Use the legacy placeholder
        // so that all callers see a consistent "single-tenant" namespace.
        Session::new_with_user(
            "test-session".to_string(),
            crate::store::SERVER_DEFAULT_USER_ID.to_string(),
        )
        .expect("SERVER_DEFAULT_USER_ID is non-empty")
    }

    // --- State machine integration tests ---

    #[test]
    fn test_state_machine_initial_state() {
        let (sm, _temp) = make_state_machine();
        assert_eq!(sm.current_state(), SessionState::Initializing);
    }

    #[test]
    fn test_state_machine_valid_transition() {
        let (mut sm, _temp) = make_state_machine();
        let mut session = make_session();
        let effect =
            sm.transition_to(SessionState::WaitingForInput, &mut session);
        assert!(effect.is_ok());
        assert_eq!(sm.current_state(), SessionState::WaitingForInput);
    }

    #[test]
    fn test_state_machine_invalid_transition() {
        let (mut sm, _temp) = make_state_machine();
        let mut session = make_session();
        let result = sm.transition_to(SessionState::Completed, &mut session);
        assert!(result.is_err());
        assert_eq!(sm.current_state(), SessionState::Initializing);
    }

    #[test]
    fn test_state_machine_transition_persists_metadata() {
        let (mut sm, _temp) = make_state_machine();
        let mut session = make_session();
        sm.transition_to(SessionState::WaitingForInput, &mut session)
            .unwrap();

        // Verify metadata was persisted with the new state
        let metadata = sm
            .session_store
            .load_metadata("_legacy_", "test-session")
            .unwrap();
        assert_eq!(metadata.state, SessionState::WaitingForInput);
    }

    #[test]
    fn test_state_machine_full_lifecycle() {
        let (mut sm, _temp) = make_state_machine();
        let mut session = make_session();

        // Initializing -> WaitingForInput
        sm.transition_to(SessionState::WaitingForInput, &mut session)
            .unwrap();
        assert_eq!(sm.current_state(), SessionState::WaitingForInput);

        // WaitingForInput -> LlmCalling
        sm.transition_to(SessionState::LlmCalling, &mut session)
            .unwrap();
        assert_eq!(sm.current_state(), SessionState::LlmCalling);

        // LlmCalling -> ToolScheduling
        sm.transition_to(SessionState::ToolScheduling, &mut session)
            .unwrap();
        assert_eq!(sm.current_state(), SessionState::ToolScheduling);

        // ToolScheduling -> WaitingForInput
        sm.transition_to(SessionState::WaitingForInput, &mut session)
            .unwrap();
        assert_eq!(sm.current_state(), SessionState::WaitingForInput);

        // LlmCalling -> Completed
        sm.transition_to(SessionState::LlmCalling, &mut session)
            .unwrap();
        sm.transition_to(SessionState::Completed, &mut session)
            .unwrap();
        assert_eq!(sm.current_state(), SessionState::Completed);

        // Completed -> Initializing (reset)
        sm.transition_to(SessionState::Initializing, &mut session)
            .unwrap();
        assert_eq!(sm.current_state(), SessionState::Initializing);
    }

    #[test]
    fn test_state_machine_approval_timeout_effect() {
        let (mut sm, _temp) = make_state_machine();
        let mut session = make_session();

        // Transition to WaitingForApproval should return StartApprovalTimeout effect
        let effect =
            sm.transition_to(SessionState::WaitingForApproval, &mut session);
        assert!(effect.is_ok());
        assert!(matches!(
            effect.unwrap(),
            StateEnterEffect::StartApprovalTimeout
        ));
    }

    #[test]
    fn test_state_machine_cancel_approval_effect() {
        let (mut sm, _temp) = make_state_machine();
        let mut session = make_session();

        // First go to WaitingForApproval
        sm.transition_to(SessionState::WaitingForApproval, &mut session)
            .unwrap();

        // Then to ToolScheduling should return CancelApprovalTimeout effect
        let effect =
            sm.transition_to(SessionState::ToolScheduling, &mut session);
        assert!(effect.is_ok());
        assert!(matches!(
            effect.unwrap(),
            StateEnterEffect::CancelApprovalTimeout
        ));
    }

    #[test]
    fn test_state_machine_no_effect_for_normal_transitions() {
        let (mut sm, _temp) = make_state_machine();
        let mut session = make_session();

        // Initializing -> WaitingForInput returns CancelApprovalTimeout, not None
        // Test a transition that returns None: Compacting -> WaitingForInput
        // First go to Compacting (valid from any state)
        sm.transition_to(SessionState::Compacting, &mut session)
            .unwrap();

        // Compacting -> WaitingForInput returns CancelApprovalTimeout
        // Test Completed -> Initializing which returns None
        sm.transition_to(SessionState::WaitingForInput, &mut session)
            .unwrap();
        sm.transition_to(SessionState::LlmCalling, &mut session)
            .unwrap();
        sm.transition_to(SessionState::Completed, &mut session)
            .unwrap();

        // Completed -> Initializing returns None
        let effect = sm.transition_to(SessionState::Initializing, &mut session);
        assert!(effect.is_ok());
        assert!(matches!(effect.unwrap(), StateEnterEffect::None));
    }
}
