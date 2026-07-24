//! Tool-message truncation step.
//!
//! The truncate service is destructive on `ctx.messages` (in place):
//! head/tail are kept, the full content is spilled to disk under
//! the unified `synthia_context::truncate` contract. Only `Tool`
//! role messages are touched — system / user / assistant messages
//! pass through byte-identical so the LLM still sees the full
//! conversation history for non-tool roles.
//!
//! Kept separate from [`super::core`] (the `StepSample` orchestrator)
//! and [`super::request`] (the `CompletionRequest` builder) so the
//! truncate policy is one self-contained helper that can be
//! unit-tested in isolation if needed.

use synthia_context::truncate::{TruncateConfig, truncate_messages};
use synthia_provider::{Message, Role};

/// Apply the unified truncate service to Tool-role messages BEFORE
/// the LLM call. Returns the number of messages that were actually
/// truncated (mostly for the `debug!` log; the message slice is
/// modified in place either way).
pub(super) fn truncate_tool_messages(
    messages: &mut [Message],
    cfg: &TruncateConfig,
) -> usize {
    let truncated = truncate_messages(messages, cfg, |m| m.role == Role::Tool);
    let n = truncated.len();
    if n > 0 {
        tracing::debug!(
            target: "synthia.agent.step_sample",
            truncated = n,
            "Applied Truncate service to Tool role messages before LLM call",
        );
    }
    n
}
