//! Wire-level error types (non_exhaustive for future expansion).

use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ProtocolError {
    #[error("malformed JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("unknown op variant: {0}")]
    UnknownOp(String),

    #[error("unknown event variant: {0}")]
    UnknownEvent(String),

    #[error("invalid W3C traceparent: {0}")]
    InvalidTraceparent(String),

    #[error("schema version mismatch: expected {expected}, got {got}")]
    VersionMismatch { expected: u32, got: u32 },

    #[error("invalid approval request: {0}")]
    InvalidApproval(String),
}

pub type Result<T> = std::result::Result<T, ProtocolError>;
