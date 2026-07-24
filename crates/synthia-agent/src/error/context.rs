//! `ProviderErrorContext` trait impls + `From` bridge into
//! [`AgentError`].
//!
//! The struct itself is defined in [`super::core`]; this
//! submodule owns:
//!
//! - [`Display`] for [`ProviderErrorContext`] — prefixes
//!   `[status=NNN]` and/or `[non-retryable]` markers when
//!   those fields are set, then delegates to the underlying
//!   `ProviderError`'s `Display`.
//! - [`std::error::Error`] for [`ProviderErrorContext`] —
//!   `source()` returns the wrapped `ProviderError` so the
//!   `std::error::Error` chain stays intact.
//! - `From<ProviderErrorContext> for AgentError` — bridges
//!   the context-wrapped form into the [`super::core::AgentError`]
//!   enum's `ProviderWithContext` variant.
//!
//! Kept separate from [`super::core`] so the 21-variant
//! error enum doesn't get buried under provider-specific
//! formatting concerns.

use super::core::{AgentError, ProviderErrorContext};

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
