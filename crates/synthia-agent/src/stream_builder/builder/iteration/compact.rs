//! Compaction helpers for the per-iteration loop.

use super::types::CompactOutcome;
use crate::{
    config::AgentConfig,
    loop_context::LoopContext,
    stream_builder::steps::{CompactAction, StepCompact},
};

/// Check the compact step and run it if necessary.
///
/// Caller observes the variant and yields the
/// appropriate `TokenBudgetWarning` /
/// `ContextCompacted` events.
pub(crate) fn do_compact_step(
    step: &StepCompact,
    ctx: &mut LoopContext,
    config: &AgentConfig,
) -> CompactOutcome {
    match step.check(ctx, config) {
        CompactAction::MustCompact => {
            if let Some(result) = step.execute(ctx, config) {
                CompactOutcome::MustCompact {
                    old_tokens: result.old_tokens,
                    new_tokens: result.new_tokens,
                }
            } else {
                CompactOutcome::None
            }
        }
        CompactAction::Warning => CompactOutcome::Warning,
        CompactAction::None => CompactOutcome::None,
    }
}
