//! The 10-state [`SessionState`] enum + the
//! [`InvalidStateTransition`] error struct that the state
//! machine surfaces when [`crate::state_machine::is_valid_transition`]
//! rejects a transition.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    Initializing,
    WaitingForInput,
    LlmCalling,
    ToolScheduling,
    Compacting,
    WaitingForApproval,
    Paused,
    Completed,
    Cancelled,
    Error,
}

#[derive(Error, Debug)]
#[error("Invalid state transition: {from:?} to {to:?}")]
pub struct InvalidStateTransition {
    pub from: SessionState,
    pub to: SessionState,
}
