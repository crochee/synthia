use async_trait::async_trait;
use synthia_provider::Message;

use crate::types::ContextError;

/// Trait for compacting messages using an LLM.
/// Separated from ModelProvider to avoid a hard dependency in this crate.
///
/// `generate_summary` receives an optional `previous_summary` — when
/// `Some(_)`, the provider is expected to inject the
/// `<previous-summary>{summary}</previous-summary>` anchor into its prompt
/// so the new summary is built on top of the old one (decision continuity).
/// When `None`, the provider starts a fresh anchor.
#[async_trait]
pub trait CompactionProvider: Send + Sync {
    async fn generate_summary(
        &self,
        messages: &[Message],
        previous_summary: Option<&str>,
    ) -> Result<String, ContextError>;
}
