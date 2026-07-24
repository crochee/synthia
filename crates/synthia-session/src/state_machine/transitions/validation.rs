//! Pure transition-validation logic: which states can move to which.

use crate::types::SessionState;

/// Validates whether a state transition is allowed.
/// Extracted from types.rs to centralize state machine logic.
pub fn is_valid_transition(from: SessionState, to: SessionState) -> bool {
    matches!(
        (from, to),
        (SessionState::Initializing, SessionState::WaitingForInput)
            | (SessionState::WaitingForInput, SessionState::LlmCalling)
            | (SessionState::WaitingForInput, SessionState::Paused)
            | (SessionState::WaitingForInput, SessionState::Cancelled)
            | (SessionState::LlmCalling, SessionState::ToolScheduling)
            | (SessionState::LlmCalling, SessionState::WaitingForInput)
            | (SessionState::LlmCalling, SessionState::Completed)
            | (SessionState::LlmCalling, SessionState::Cancelled)
            | (SessionState::LlmCalling, SessionState::Error)
            | (SessionState::ToolScheduling, SessionState::WaitingForInput)
            | (SessionState::ToolScheduling, SessionState::Cancelled)
            | (SessionState::ToolScheduling, SessionState::Error)
            | (_, SessionState::Compacting)
            | (SessionState::Compacting, SessionState::WaitingForInput)
            | (_, SessionState::WaitingForApproval)
            | (
                SessionState::WaitingForApproval,
                SessionState::ToolScheduling
            )
            | (SessionState::WaitingForApproval, SessionState::Cancelled)
            | (SessionState::WaitingForApproval, SessionState::Error)
            | (SessionState::Paused, SessionState::WaitingForInput)
            | (SessionState::Completed, SessionState::Initializing)
            | (SessionState::Cancelled, SessionState::Initializing)
            | (SessionState::Error, SessionState::Initializing)
    )
}
