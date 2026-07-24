//! Side-effect determination for state transitions.

use crate::types::SessionState;

/// Defines side effects that the caller should handle after a state transition.
#[derive(Debug, Clone, Copy)]
pub enum StateEnterEffect {
    /// Start the approval timeout timer for the given session.
    StartApprovalTimeout,
    /// Cancel the approval timeout timer for the given session.
    CancelApprovalTimeout,
    /// No external side effect needed.
    None,
}

/// Determines what side effect should be triggered when entering a state.
pub fn effect_for_entering(state: SessionState) -> StateEnterEffect {
    match state {
        SessionState::WaitingForApproval => {
            StateEnterEffect::StartApprovalTimeout
        }
        SessionState::ToolScheduling | SessionState::WaitingForInput => {
            StateEnterEffect::CancelApprovalTimeout
        }
        _ => StateEnterEffect::None,
    }
}
