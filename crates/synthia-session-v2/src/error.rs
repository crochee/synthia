//! Session storage error types.

use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SessionError {
    #[error("io: {0}")]
    Io(#[from] tokio::io::Error),

    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("tree operation failed: {0}")]
    Tree(String),

    #[error("writer channel closed")]
    WriterClosed,
}

pub type Result<T> = std::result::Result<T, SessionError>;
