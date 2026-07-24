//! Core error type definitions for `synthia-agent`.
//!
//! This submodule owns the two data carriers that the rest of the
//! error family builds on:
//!
//! - [`AgentError`]: the 21-variant `#[derive(Error)]` enum
//!   that covers every failure mode the agent can encounter
//!   (provider calls, tool execution, session/context/config
//!   management, I/O, JSON, MCP, database, validation, rate
//!   limits, context-window overflow, file conflicts, guardian
//!   vetoes, cancellation, and pool shutdown). All `From`
//!   conversions, constructor helpers, and predicate methods
//!   live in sibling submodules and `impl` blocks.
//! - [`ProviderErrorContext`]: the structured carrier for
//!   provider errors that need to surface an HTTP status code
//!   and a retryable flag. The `Display` / `Error` impls
//!   for this struct live in `super::context`; the
//!   `From<ProviderErrorContext> for AgentError` impl lives
//!   there too.
//!
//! Kept as the single source of truth for the error enum so
//! other modules never have to reconstruct the variants.

use std::io;

use synthia_provider::ProviderError;
use thiserror::Error;

/// Main error type for agent operations.
///
/// This enum covers all possible errors that can occur during agent execution,
/// including provider errors, tool errors, session errors, and more.
#[derive(Error, Debug)]
pub enum AgentError {
    /// Provider-related errors (API calls, model issues, etc.)
    #[error("Provider error: {0}")]
    Provider(#[from] ProviderError),

    /// Structured provider error with context for API errors with status codes
    /// and retryable flags. Chains to the underlying ProviderError via
    /// `#[error(transparent)]`.
    #[error(transparent)]
    ProviderWithContext(ProviderErrorContext),

    /// Tool execution errors
    #[error("Tool '{tool}' failed: {message}")]
    ToolError { tool: String, message: String },

    /// Tool approval required
    #[error("Tool approval required: {0}")]
    ToolApprovalRequired(String),

    /// Session management errors
    #[error("Session error: {0}")]
    SessionError(String),

    /// Context management errors
    #[error("Context error: {0}")]
    ContextError(String),

    /// Configuration errors
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// I/O errors
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),

    /// JSON serialization/deserialization errors
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    /// MCP server errors
    #[error("MCP server error: {0}")]
    McpServerError(String),

    /// Database errors
    #[error("Database error: {0}")]
    DatabaseError(String),

    /// Validation errors
    #[error("Validation error: {0}")]
    ValidationError(String),

    /// Invalid operation errors
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    /// Timeout errors
    #[error("Operation timeout: {0}")]
    Timeout(String),

    /// Rate limiting errors
    #[error("Rate limited, retry after {retry_after:?} seconds")]
    RateLimited { retry_after: Option<u64> },

    /// Context window exceeded errors
    #[error("Context window exceeded: current {current} tokens, limit {limit}")]
    ContextWindowExceeded { current: usize, limit: usize },

    /// File conflict errors
    #[error("File conflict: {path} has been modified")]
    FileConflict { path: String },

    /// Internal errors (catch-all)
    #[error("Internal error: {0}")]
    InternalError(String),

    /// Guardian denied the action
    #[error("Guardian denied: {0}")]
    GuardianDenied(String),

    /// Operation was cancelled
    #[error("Operation cancelled")]
    Cancelled,

    /// Pool closed errors
    #[error("Pool closed: {0}")]
    PoolClosed(String),
}

/// Structured context for provider errors with status codes and retry metadata.
#[derive(Debug)]
pub struct ProviderErrorContext {
    /// The underlying provider error.
    pub error: ProviderError,
    /// HTTP status code if available (e.g., 429, 500, 503).
    pub status_code: Option<u16>,
    /// Whether this error should be retried based on its nature.
    pub retryable: bool,
}
