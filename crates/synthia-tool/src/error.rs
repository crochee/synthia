//! Error types for tool execution.

use thiserror::Error;

/// Errors that can occur during tool execution.
#[derive(Debug, Error)]
pub enum ToolError {
    /// The tool operation was cancelled via a [`CancellationToken`].
    #[error("tool execution was cancelled")]
    Cancelled,

    /// Tool execution failed.
    #[error("tool execution failed: {0}")]
    Execution(String),

    /// Internal tool error.
    #[error("internal tool error: {0}")]
    Internal(String),
}
