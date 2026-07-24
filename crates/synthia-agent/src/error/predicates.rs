//! `AgentError` predicate methods.
//!
//! Four `is_*` methods that classify an `AgentError`
//! instance without forcing callers to write a
//! `matches!()` every time:
//!
//! - [`is_timeout`](AgentError::is_timeout) — true for
//!   the `Timeout` variant.
//! - [`is_rate_limited`](AgentError::is_rate_limited) —
//!   true for the `RateLimited` variant.
//! - [`is_context_window_exceeded`](AgentError::is_context_window_exceeded) —
//!   true for the `ContextWindowExceeded` variant.
//! - [`is_retryable`](AgentError::is_retryable) — true
//!   for transient errors (timeouts, rate limits,
//!   cancellations, retryable provider errors). The
//!   `ProviderErrorContext` carries its own `retryable`
//!   flag, so the structured variant honors that.
//!
//! Kept separate from [`super::constructors`] (which
//! *build* errors) and [`super::core`] (which *defines*
//! them) so the classification logic can evolve on its
//! own — e.g. adding `is_auth_failure()` won't touch
//! the constructors or the enum definition.

use synthia_provider::ProviderError;

use super::core::AgentError;

impl AgentError {
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
        matches!(
            self,
            Self::Timeout(_) | Self::RateLimited { .. } | Self::Cancelled
        ) || match self {
            Self::Provider(err) => {
                matches!(
                    err,
                    ProviderError::HttpError(_)
                        | ProviderError::RateLimitError(_)
                        | ProviderError::Timeout
                )
            }
            Self::ProviderWithContext(ctx) => ctx.retryable,
            _ => false,
        }
    }
}
