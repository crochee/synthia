//! `From` trait impls that lift foreign error types into
//! [`AgentError`].
//!
//! Six conversions live here, each routing to the
//! variant that best matches the source error's semantics:
//!
//! - [`String`] / [`&str`] → `AgentError::InternalError` —
//!   catch-all for ad-hoc stringly-typed errors.
//! - [`tokio::task::JoinError`] → `AgentError::InternalError` —
//!   background-task panics surface as internal errors.
//! - [`synthia_core::Error`] → `AgentError::Provider(...)`
//!   for the `Provider` variant, otherwise
//!   `AgentError::InternalError`. The provider arm
//!   re-wraps the message as a `ProviderError::api` so
//!   the structured retry classification in
//!   [`super::predicates::is_retryable`] still works.
//! - [`synthia_session::session::SessionError`] →
//!   `AgentError::InternalError` (stringified).
//! - [`anyhow::Error`] → `AgentError::InternalError`
//!   (stringified) — last-resort catch-all for code that
//!   uses `anyhow!` before crossing the agent boundary.
//!
//! Kept separate from [`super::core`] (the enum itself)
//! and from [`super::context`] (the
//! `ProviderErrorContext` → `AgentError` bridge) so the
//! foreign-type boundary is in one place.

use synthia_provider::ProviderError;

use super::core::AgentError;

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

impl From<synthia_core::Error> for AgentError {
    fn from(e: synthia_core::Error) -> Self {
        match e {
            synthia_core::Error::Provider(msg) => {
                AgentError::Provider(ProviderError::api(&msg))
            }
            _ => AgentError::InternalError(e.to_string()),
        }
    }
}

impl From<synthia_session::session::SessionError> for AgentError {
    fn from(e: synthia_session::session::SessionError) -> Self {
        AgentError::InternalError(e.to_string())
    }
}

impl From<anyhow::Error> for AgentError {
    fn from(e: anyhow::Error) -> Self {
        AgentError::InternalError(e.to_string())
    }
}
