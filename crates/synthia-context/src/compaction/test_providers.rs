//! Shared test-only `CompactionProvider` implementations.
//!
//! These are reachable from any `#[cfg(test)]` module inside
//! `crate::compaction` (i.e. the per-level unit tests in
//! `level1::tests`, `compactor::tests`, and `orchestrator::tests`).
//! The visibility is `pub(crate)` so they stay internal to the
//! `synthia-context` crate; downstream integration tests that
//! need their own `CompactionProvider` should define one locally
//! (see `synthia-agent/tests/test_support.rs` for the pattern).
//!
//! Centralising these avoids the historical pattern of inlining
//! `ConstantProvider` / `FailingProvider` / `EmptyProvider` in
//! every test file, which made the per-level modules depend on
//! each other's `tests` submodule — a layering violation that
//! surfaced when the file split exposed the broken imports.

use async_trait::async_trait;
use synthia_provider::Message;

use super::level1::CompactionProvider;
use crate::types::ContextError;

/// Captures the `previous_summary` argument the provider actually
/// sees, so tests can assert on anchor forwarding / truncation
/// without re-running the LLM.
pub struct CapturingProvider {
    pub last_previous: parking_lot::Mutex<Option<String>>,
    pub summary: String,
}

#[async_trait]
impl CompactionProvider for CapturingProvider {
    async fn generate_summary(
        &self,
        _messages: &[Message],
        previous_summary: Option<&str>,
    ) -> Result<String, ContextError> {
        *self.last_previous.lock() = previous_summary.map(|s| s.to_string());
        Ok(self.summary.clone())
    }
}

/// Returns the same fixed summary on every call (no anchor capture).
pub struct ConstantProvider(pub String);

#[async_trait]
impl CompactionProvider for ConstantProvider {
    async fn generate_summary(
        &self,
        _messages: &[Message],
        _previous_summary: Option<&str>,
    ) -> Result<String, ContextError> {
        Ok(self.0.clone())
    }
}

/// Always fails with a `Checkpoint` error.
pub struct FailingProvider;

#[async_trait]
impl CompactionProvider for FailingProvider {
    async fn generate_summary(
        &self,
        _messages: &[Message],
        _previous_summary: Option<&str>,
    ) -> Result<String, ContextError> {
        Err(ContextError::Checkpoint("provider unavailable".into()))
    }
}

/// Returns an empty summary (forcing the structured-fallback path).
pub struct EmptyProvider;

#[async_trait]
impl CompactionProvider for EmptyProvider {
    async fn generate_summary(
        &self,
        _messages: &[Message],
        _previous_summary: Option<&str>,
    ) -> Result<String, ContextError> {
        Ok(String::new())
    }
}
