//! Error type for state machine operations.

use thiserror::Error;

use crate::types::InvalidStateTransition;

/// Error type for state machine operations.
#[derive(Error, Debug)]
pub enum StateMachineError {
    #[error(transparent)]
    InvalidTransition(#[from] InvalidStateTransition),
    #[error("state machine persistence error: {0}")]
    Persistence(#[from] anyhow::Error),
}
