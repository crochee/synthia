//! Error types for synthia-agent
//!
//! This module defines all error types used throughout the agent system.
//! It provides a unified error handling mechanism with rich context support.

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
    McpServerError(#[from] rmcp::ServiceError),

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

impl std::fmt::Display for ProviderErrorContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(code) = self.status_code {
            write!(f, "[status={code}] ")?;
        }
        if !self.retryable {
            write!(f, "[non-retryable] ")?;
        }
        std::fmt::Display::fmt(&self.error, f)
    }
}

impl std::error::Error for ProviderErrorContext {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

impl From<ProviderErrorContext> for AgentError {
    fn from(ctx: ProviderErrorContext) -> Self {
        Self::ProviderWithContext(ctx)
    }
}

impl AgentError {
    /// Creates a tool error with the given tool name and message.
    ///
    /// # Examples
    ///
    /// ```
    /// use synthia_agent::AgentError;
    ///
    /// let error = AgentError::tool("read_file", "file not found");
    /// ```
    pub fn tool(tool: impl Into<String>, message: impl Into<String>) -> Self {
        Self::ToolError {
            tool: tool.into(),
            message: message.into(),
        }
    }

    /// Creates a tool error with an unknown tool name.
    ///
    /// # Examples
    ///
    /// ```
    /// use synthia_agent::AgentError;
    ///
    /// let error = AgentError::tool_error("something went wrong");
    /// ```
    pub fn tool_error(message: impl Into<String>) -> Self {
        Self::ToolError {
            tool: "unknown".into(),
            message: message.into(),
        }
    }

    /// Creates a session error.
    ///
    /// # Examples
    ///
    /// ```
    /// use synthia_agent::AgentError;
    ///
    /// let error = AgentError::session("session not found");
    /// ```
    pub fn session(message: impl Into<String>) -> Self {
        Self::SessionError(message.into())
    }

    /// Creates a context error.
    ///
    /// # Examples
    ///
    /// ```
    /// use synthia_agent::AgentError;
    ///
    /// let error = AgentError::context("context window exceeded");
    /// ```
    pub fn context(message: impl Into<String>) -> Self {
        Self::ContextError(message.into())
    }

    /// Creates a configuration error.
    ///
    /// # Examples
    ///
    /// ```
    /// use synthia_agent::AgentError;
    ///
    /// let error = AgentError::config("missing required field");
    /// ```
    pub fn config(message: impl Into<String>) -> Self {
        Self::ConfigError(message.into())
    }

    /// Creates a validation error.
    ///
    /// # Examples
    ///
    /// ```
    /// use synthia_agent::AgentError;
    ///
    /// let error = AgentError::validation("invalid input");
    /// ```
    pub fn validation(message: impl Into<String>) -> Self {
        Self::ValidationError(message.into())
    }

    /// Creates a timeout error.
    ///
    /// # Examples
    ///
    /// ```
    /// use synthia_agent::AgentError;
    ///
    /// let error = AgentError::timeout("operation took too long");
    /// ```
    pub fn timeout(message: impl Into<String>) -> Self {
        Self::Timeout(message.into())
    }

    /// Creates an internal error.
    ///
    /// # Examples
    ///
    /// ```
    /// use synthia_agent::AgentError;
    ///
    /// let error = AgentError::internal("unexpected state");
    /// ```
    pub fn internal(message: impl Into<String>) -> Self {
        Self::InternalError(message.into())
    }

    /// Creates a database error.
    ///
    /// # Examples
    ///
    /// ```
    /// use synthia_agent::AgentError;
    ///
    /// let error = AgentError::database("connection failed");
    /// ```
    pub fn database(message: impl Into<String>) -> Self {
        Self::DatabaseError(message.into())
    }

    /// Creates a pool closed error.
    pub fn pool_closed(message: impl Into<String>) -> Self {
        Self::PoolClosed(message.into())
    }

    /// Creates a file conflict error.
    ///
    /// # Examples
    ///
    /// ```
    /// use synthia_agent::AgentError;
    ///
    /// let error = AgentError::file_conflict("/path/to/file");
    /// ```
    pub fn file_conflict(path: impl Into<String>) -> Self {
        Self::FileConflict { path: path.into() }
    }

    /// Creates a rate limited error.
    ///
    /// # Examples
    ///
    /// ```
    /// use synthia_agent::AgentError;
    ///
    /// let error = AgentError::rate_limited(Some(60));
    /// ```
    pub fn rate_limited(retry_after: Option<u64>) -> Self {
        Self::RateLimited { retry_after }
    }

    /// Creates a context window exceeded error.
    ///
    /// # Examples
    ///
    /// ```
    /// use synthia_agent::AgentError;
    ///
    /// let error = AgentError::context_window_exceeded(10000, 8000);
    /// ```
    pub fn context_window_exceeded(current: usize, limit: usize) -> Self {
        Self::ContextWindowExceeded { current, limit }
    }

    /// Returns true if this error is a timeout error.
    pub fn is_timeout(&self) -> bool {
        matches!(self, Self::Timeout(_))
    }

    /// Returns true if this error is a rate limit error.
    pub fn is_rate_limited(&self) -> bool {
        matches!(self, Self::RateLimited { .. })
    }

    /// Returns true if this error is a context window exceeded error.
    pub fn is_context_window_exceeded(&self) -> bool {
        matches!(self, Self::ContextWindowExceeded { .. })
    }

    /// Returns true if this error is retryable.
    ///
    /// Retries are appropriate for: timeout errors, rate limiting errors,
    /// and transient provider errors (HTTP errors, rate limit errors, cancellations).
    /// Non-retryable errors include: authentication failures, context window
    /// exceeded, and most validation errors.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Timeout(_))
            || matches!(self, Self::RateLimited { .. })
            || matches!(self, Self::Cancelled)
            || match self {
                Self::Provider(err) => {
                    matches!(
                        err,
                        ProviderError::HttpError(_)
                            | ProviderError::RateLimitError(_)
                            | ProviderError::Timeout
                            | ProviderError::Cancelled
                    )
                }
                Self::ProviderWithContext(ctx) => ctx.retryable,
                _ => false,
            }
    }

    /// Creates a provider error with structured context including status code
    /// and retryable flag.
    ///
    /// # Examples
    ///
    /// ```
    /// use synthia_agent::error::{AgentError, ProviderErrorContext};
    /// use synthia_provider::ProviderError;
    ///
    /// let ctx = ProviderErrorContext {
    ///     error: ProviderError::rate_limit("rate limited"),
    ///     status_code: Some(429),
    ///     retryable: true,
    /// };
    /// let error = AgentError::provider_with_context(ctx);
    /// assert!(error.is_retryable());
    /// ```
    pub fn provider_with_context(ctx: ProviderErrorContext) -> Self {
        Self::ProviderWithContext(ctx)
    }
}

impl From<String> for AgentError {
    fn from(e: String) -> Self {
        AgentError::InternalError(e)
    }
}

impl From<&str> for AgentError {
    fn from(e: &str) -> Self {
        AgentError::InternalError(e.to_string())
    }
}

impl From<tokio::task::JoinError> for AgentError {
    fn from(e: tokio::task::JoinError) -> Self {
        AgentError::InternalError(e.to_string())
    }
}

impl From<sqlx::Error> for AgentError {
    fn from(e: sqlx::Error) -> Self {
        AgentError::DatabaseError(e.to_string())
    }
}

impl From<regex::Error> for AgentError {
    fn from(e: regex::Error) -> Self {
        AgentError::InternalError(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::*;

    #[test]
    fn test_tool_error() {
        let error = AgentError::tool("my_tool", "failed");
        match error {
            AgentError::ToolError { tool, message } => {
                assert_eq!(tool, "my_tool");
                assert_eq!(message, "failed");
            }
            _ => panic!("Expected ToolError"),
        }
    }

    #[test]
    fn test_tool_error_convenience() {
        let error = AgentError::tool_error("something failed");
        match error {
            AgentError::ToolError { tool, message } => {
                assert_eq!(tool, "unknown");
                assert_eq!(message, "something failed");
            }
            _ => panic!("Expected ToolError"),
        }
    }

    #[test]
    fn test_from_string() {
        let error = AgentError::from("test error");
        match error {
            AgentError::InternalError(msg) => assert_eq!(msg, "test error"),
            _ => panic!("Expected InternalError"),
        }
    }

    #[test]
    fn test_from_provider_error() {
        let provider_error = ProviderError::api("API error");
        let agent_error = AgentError::from(provider_error);
        assert!(matches!(agent_error, AgentError::Provider(_)));
    }

    #[test]
    fn test_error_message_formatting() {
        let error = AgentError::tool("tool", "message");
        assert!(error.to_string().contains("tool"));
        assert!(error.to_string().contains("message"));
    }

    #[test]
    fn test_session_error() {
        let error = AgentError::session("session not found");
        assert!(matches!(error, AgentError::SessionError(_)));
        assert!(error.to_string().contains("session not found"));
    }

    #[test]
    fn test_context_error() {
        let error = AgentError::context("context overflow");
        assert!(matches!(error, AgentError::ContextError(_)));
        assert!(error.to_string().contains("context overflow"));
    }

    #[test]
    fn test_config_error() {
        let error = AgentError::config("missing field");
        assert!(matches!(error, AgentError::ConfigError(_)));
        assert!(error.to_string().contains("missing field"));
    }

    #[test]
    fn test_validation_error() {
        let error = AgentError::validation("invalid input");
        assert!(matches!(error, AgentError::ValidationError(_)));
        assert!(error.to_string().contains("invalid input"));
    }

    #[test]
    fn test_timeout_error() {
        let error = AgentError::timeout("operation timed out");
        assert!(error.is_timeout());
        assert!(error.to_string().contains("operation timed out"));
    }

    #[test]
    fn test_internal_error() {
        let error = AgentError::internal("unexpected");
        assert!(matches!(error, AgentError::InternalError(_)));
        assert!(error.to_string().contains("unexpected"));
    }

    #[test]
    fn test_database_error() {
        let error = AgentError::database("connection failed");
        assert!(matches!(error, AgentError::DatabaseError(_)));
        assert!(error.to_string().contains("connection failed"));
    }

    #[test]
    fn test_file_conflict_error() {
        let error = AgentError::file_conflict("/path/to/file");
        assert!(matches!(error, AgentError::FileConflict { .. }));
        assert!(error.to_string().contains("/path/to/file"));
    }

    #[test]
    fn test_rate_limited_error() {
        let error = AgentError::rate_limited(Some(60));
        assert!(error.is_rate_limited());
        assert!(error.to_string().contains("60"));
    }

    #[test]
    fn test_context_window_exceeded_error() {
        let error = AgentError::context_window_exceeded(10000, 8000);
        assert!(error.is_context_window_exceeded());
        assert!(error.to_string().contains("10000"));
        assert!(error.to_string().contains("8000"));
    }

    #[test]
    fn test_from_sqlx_error() {
        let sqlx_error = sqlx::Error::RowNotFound;
        let agent_error = AgentError::from(sqlx_error);
        assert!(matches!(agent_error, AgentError::DatabaseError(_)));
    }

    #[test]
    fn test_from_join_error() {
        // Note: We can't easily create a JoinError, but we can test the conversion
        // This is a compile-time check that the conversion exists
        fn _check_conversion(e: tokio::task::JoinError) -> AgentError {
            AgentError::from(e)
        }
    }

    #[test]
    fn test_from_str() {
        let error: AgentError = "test string error".into();
        match error {
            AgentError::InternalError(msg) => {
                assert_eq!(msg, "test string error")
            }
            _ => panic!("Expected InternalError"),
        }
    }

    #[test]
    fn test_from_regex_error() {
        let regex_error = regex::Error::Syntax("invalid regex".to_string());
        let error: AgentError = regex_error.into();
        match error {
            AgentError::InternalError(msg) => {
                assert!(msg.contains("invalid regex"))
            }
            _ => panic!("Expected InternalError"),
        }
    }

    #[test]
    fn test_tool_approval_required_error() {
        let error = AgentError::ToolApprovalRequired("read_file".to_string());
        assert!(error.to_string().contains("Tool approval required"));
        assert!(error.to_string().contains("read_file"));
    }

    #[test]
    fn test_invalid_operation_error() {
        let error = AgentError::InvalidOperation("cannot proceed".to_string());
        assert!(error.to_string().contains("Invalid operation"));
        assert!(error.to_string().contains("cannot proceed"));
    }

    #[test]
    fn test_guardian_denied_error() {
        let error =
            AgentError::GuardianDenied("action not allowed".to_string());
        assert!(error.to_string().contains("Guardian denied"));
        assert!(error.to_string().contains("action not allowed"));
    }

    #[test]
    fn test_cancelled_error() {
        let error = AgentError::Cancelled;
        assert!(error.to_string().contains("Operation cancelled"));
        assert!(error.is_retryable());
    }

    #[test]
    fn test_pool_closed_error() {
        let error = AgentError::pool_closed("connection pool terminated");
        match error {
            AgentError::PoolClosed(ref msg) => {
                assert_eq!(msg, "connection pool terminated")
            }
            _ => panic!("Expected PoolClosed"),
        }
        assert!(error.to_string().contains("Pool closed"));
    }

    #[test]
    fn test_provider_with_context() {
        let ctx = ProviderErrorContext {
            error: ProviderError::rate_limit("rate limited"),
            status_code: Some(429),
            retryable: true,
        };
        let error = AgentError::provider_with_context(ctx);
        assert!(matches!(error, AgentError::ProviderWithContext(_)));
        assert!(error.is_retryable());
    }

    #[test]
    fn test_provider_with_context_non_retryable() {
        let ctx = ProviderErrorContext {
            error: ProviderError::api("unauthorized"),
            status_code: Some(401),
            retryable: false,
        };
        let error = AgentError::provider_with_context(ctx);
        assert!(!error.is_retryable());
        assert!(error.to_string().contains("[status=401]"));
        assert!(error.to_string().contains("[non-retryable]"));
    }

    #[test]
    fn test_provider_error_context_display_no_status() {
        let ctx = ProviderErrorContext {
            error: ProviderError::api("error"),
            status_code: None,
            retryable: true,
        };
        let display = ctx.to_string();
        assert!(!display.contains("[status="));
        assert!(display.contains("error"));
    }

    #[test]
    fn test_provider_error_context_source() {
        let ctx = ProviderErrorContext {
            error: ProviderError::Timeout,
            status_code: Some(408),
            retryable: true,
        };
        let source = ctx.source();
        assert!(source.is_some());
    }

    #[test]
    fn test_from_provider_error_context() {
        let ctx = ProviderErrorContext {
            error: ProviderError::api("api error"),
            status_code: Some(500),
            retryable: false,
        };
        let error: AgentError = ctx.into();
        assert!(matches!(error, AgentError::ProviderWithContext(_)));
    }

    #[test]
    fn test_is_retryable_for_cancelled() {
        let error = AgentError::Cancelled;
        assert!(error.is_retryable());
    }

    #[test]
    fn test_is_retryable_for_rate_limited() {
        let error = AgentError::rate_limited(None);
        assert!(error.is_retryable());
    }

    #[test]
    fn test_is_retryable_for_timeout() {
        let error = AgentError::timeout("too long");
        assert!(error.is_retryable());
    }

    #[test]
    fn test_is_retryable_for_non_retryable_errors() {
        assert!(!AgentError::tool("t", "m").is_retryable());
        assert!(!AgentError::config("c").is_retryable());
        assert!(!AgentError::validation("v").is_retryable());
        assert!(!AgentError::context_window_exceeded(100, 50).is_retryable());
    }

    #[test]
    fn test_is_not_timeout() {
        let error = AgentError::tool("t", "m");
        assert!(!error.is_timeout());
    }

    #[test]
    fn test_is_not_rate_limited() {
        let error = AgentError::tool("t", "m");
        assert!(!error.is_rate_limited());
    }

    #[test]
    fn test_is_not_context_window_exceeded() {
        let error = AgentError::tool("t", "m");
        assert!(!error.is_context_window_exceeded());
    }
}
