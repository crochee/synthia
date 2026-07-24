//! L4 auto-compact step.
//!
//! [`try_l4_compact`] gates the L4 step on the
//! [`super::core::COMPACT_THRESHOLD`] utilization ratio;
//! if the ratio is below the threshold it short-circuits
//! to `None` (the orchestrator then moves on to L5).
//!
//! On a high-ratio context the helper:
//!
//! 1. Captures the pre-compaction token count via
//!    [`synthia_context::traits::estimate_message_tokens`]
//!    so it can be forwarded to the inner
//!    `compact_level1` (FU.2: skip its own re-scan).
//! 2. Calls
//!    [`synthia_context::compaction::orchestrator::compact_with_fallback`]
//!    with the configured
//!    [`synthia_session::types::TokenBudget`] and the
//!    optional previous summary as a continuity anchor.
//! 3. Verifies the post-compaction token count is
//!    strictly lower than the pre-compaction count;
//!    if not, returns `None` (no marker, no in-place
//!    mutation, the orchestrator moves on to L5).
//! 4. On success, replaces `ctx.messages` in place,
//!    clears the `ctx.needs_compact` flag, and returns
//!    a `Context auto-compacted: N -> M tokens` marker
//!    string for the orchestrator to surface as a
//!    [`super::core::RecoveryAction::Recovered`] at
//!    level 4.
//!
//! Kept separate from [`super::l3`] and [`super::run`]
//! because the L4 step has the only async work in the
//! cascade and owns the budget / provider plumbing.

use synthia_context::{
    compaction::orchestrator::compact_with_fallback,
    traits::estimate_message_tokens,
};
use synthia_session::types::TokenBudget;

use super::core::COMPACT_THRESHOLD;
use crate::loop_context::LoopContext;

/// L4: attempts to compact `ctx.messages` via `compact_with_fallback`.
///
/// Returns `Some(marker_message)` on success (compaction reduced the
/// token count and the messages were replaced in-place), or `None` if
/// compaction should be skipped (low ratio, no budget, no reduction).
///
/// `previous_summary` is forwarded to L1 to anchor the new summary on
/// top of the prior one (decision continuity).
pub(crate) async fn try_l4_compact(
    ctx: &mut LoopContext,
    budget: &TokenBudget,
    provider: Option<
        &dyn synthia_context::compaction::level1::CompactionProvider,
    >,
    previous_summary: Option<&str>,
) -> Option<String> {
    if ctx.token_ratio() <= COMPACT_THRESHOLD {
        return None;
    }

    let target = budget.soft_limit.max(1);
    let original_tokens: usize =
        ctx.messages.iter().map(estimate_message_tokens).sum();
    let compacted = compact_with_fallback(
        &ctx.messages,
        target,
        provider,
        previous_summary,
        // FU.2: forward the value we just computed so the inner
        // `compact_level1` skips its own `estimate_tokens(msgs)` call
        // (same input, no point re-scanning).
        Some(original_tokens),
    )
    .await;
    if compacted.is_empty() {
        return None;
    }
    let compacted_tokens: usize =
        compacted.iter().map(estimate_message_tokens).sum();
    if compacted_tokens >= original_tokens {
        return None;
    }
    ctx.messages = compacted;
    ctx.needs_compact = false;
    Some(format!(
        "Context auto-compacted: {} -> {} tokens",
        original_tokens, compacted_tokens
    ))
}
