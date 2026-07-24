//! `AgentError` constructor methods.
//!
//! Each method wraps a single enum variant with sensible
//! parameter naming + `impl Into<String>` ergonomics so
//! call sites don't have to know the variant's field names.
//!
//! - Tool / session / context / config / validation /
//!   timeout / internal / database / pool-closed errors
//!   all wrap a single `String` message into the matching
//!   variant.
//! - `file_conflict` takes a path string.
//! - `rate_limited` and `context_window_exceeded` build the
//!   structured variants with their numeric fields.
//! - `provider_with_context` lifts a [`ProviderErrorContext`]
//!   into the [`AgentError::ProviderWithContext`] variant.
//! - `mcp_server` is kept in its own tiny `impl` block in
//!   the original file (line 386-391) — re-merged here so
//!   all 15 constructors sit behind one surface.
//!
//! Kept separate from [`super::core`] (the enum + struct
//! definitions) and [`super::predicates`] (the `is_*`
//! methods) so the three concerns can evolve independently.

use super::core::{AgentError, ProviderErrorContext};

impl AgentError {
    /// Creates a tool error with the given tool name and message.
    ///
    /// # Examples
    ///
    /// ```
    /// use synthia_agent::error::AgentError;
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
    /// use synthia_agent::error::AgentError;
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
    /// use synthia_agent::error::AgentError;
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
    /// use synthia_agent::error::AgentError;
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
    /// use synthia_agent::error::AgentError;
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
    /// use synthia_agent::error::AgentError;
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
    /// use synthia_agent::error::AgentError;
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
    /// use synthia_agent::error::AgentError;
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
    /// use synthia_agent::error::AgentError;
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
    /// use synthia_agent::error::AgentError;
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
    /// use synthia_agent::error::AgentError;
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
    /// use synthia_agent::error::AgentError;
    ///
    /// let error = AgentError::context_window_exceeded(10000, 8000);
    /// ```
    pub fn context_window_exceeded(current: usize, limit: usize) -> Self {
        Self::ContextWindowExceeded { current, limit }
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

    /// Creates an MCP server error from any error type.
    pub fn mcp_server(message: impl Into<String>) -> Self {
        Self::McpServerError(message.into())
    }
}
