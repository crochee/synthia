//! L3 → L4 → L5 cascade orchestrator.
//!
//! [`run_recovery_cascade`] is the public entry point
//! invoked after L1 (truncate) and L2 (retry) have
//! already failed inline. It walks three recovery
//! levels in order:
//!
//! 1. **L3 Fallback** —
//!    [`super::l3::try_l3_fallback`]. On hit, returns
//!    [`super::core::RecoveryAction::Recovered`] with
//!    `level: 3` and the fallback message.
//! 2. **L4 Auto-Compact** —
//!    [`super::l4::try_l4_compact`]. On hit, returns
//!    `Recovered` with `level: 4` and the
//!    `Context auto-compacted` marker.
//! 3. **L5 Reset** —
//!    [`crate::error_recovery::reset::ResetCoordinator::execute`]
//!    with [`ResetScope::Conversation`](crate::error_recovery::reset::ResetScope::Conversation).
//!    On hit, returns `Recovered` with `level: 5`
//!    and the reset description.
//!
//! If all three fail (L3 returns `None`, L4 returns
//! `None` or is skipped, L5 reports `success: false`
//! because the cooldown is active), the function
//! returns [`super::core::RecoveryAction::FailFast`]
//! carrying the L5 description for the caller to
//! surface.
//!
//! On every `Recovered` branch the per-tool
//! [`super::tracker::ConsecutiveFailureTracker`] is
//! cleared via `record_success` (counter reset on
//! success) and the global
//! [`crate::error_recovery::ErrorRecoveryCoordinator::record_success`]
//! is called so its consecutive-error counter and
//! cooldown timestamp both clear (L5 reset already
//! does that internally, but the helper calls it
//! defensively for the L3/L4 paths).
//!
//! Kept separate from [`super::l3`] and [`super::l4`]
//! because this is the only async + 9-argument entry
//! point; isolating the orchestrator signature from
//! the per-step helpers keeps the L3/L4 unit tests
//! trivial and the orchestrator signature stable.

use synthia_context::compaction::level1::CompactionProvider;
use synthia_guardian::LoopDetectorSet;
use synthia_session::types::TokenBudget;

use super::{
    core::RecoveryAction,
    l3::try_l3_fallback,
    l4::try_l4_compact,
    tracker::ConsecutiveFailureTracker,
};
use crate::{
    error_recovery::{
        ErrorRecoveryCoordinator,
        reset::{ResetCoordinator, ResetScope},
    },
    loop_context::LoopContext,
    steering::SteeringChannel,
};

/// Runs the L3 → L4 → L5 recovery cascade for a single tool failure.
///
/// Order:
/// 1. **L3 Fallback** — if the tool has a registered fallback strategy
///    AND it has failed 2+ times, return the fallback message.
/// 2. **L4 Auto-Compact** — if the context utilization ratio exceeds
///    `COMPACT_THRESHOLD` (0.8), call `compact_with_fallback()`. On a
///    successful reduction, return a compaction marker.
/// 3. **L5 Reset** — when L3 and L4 do not apply, call
///    `ResetCoordinator::execute(ResetScope::Conversation)`. On
///    success, the conversation is reset and `Recovered` is returned;
///    on failure (e.g. cooldown active) `FailFast` is returned.
#[allow(clippy::too_many_arguments)]
pub async fn run_recovery_cascade(
    error: &str,
    tool_name: &str,
    ctx: &mut LoopContext,
    tracker: &mut ConsecutiveFailureTracker,
    recovery: &ErrorRecoveryCoordinator,
    budget: Option<&TokenBudget>,
    provider: Option<&dyn CompactionProvider>,
    loop_detector: &mut LoopDetectorSet,
    steering: Option<&dyn SteeringChannel>,
    reset_coordinator: &ResetCoordinator,
) -> RecoveryAction {
    // --- L3: Fallback ---
    if let Some(fallback_msg) = try_l3_fallback(tool_name, tracker) {
        tracker.record_success(tool_name);
        recovery.record_success();
        tracing::info!(
            tool = %tool_name,
            error,
            "L3 fallback applied"
        );
        return RecoveryAction::Recovered {
            message: fallback_msg,
            level: 3,
        };
    }

    // --- L4: Auto-Compact ---
    if let Some(budget) = budget
        && let Some(msg) = try_l4_compact(ctx, budget, provider, None).await
    {
        tracker.record_success(tool_name);
        recovery.record_success();
        tracing::info!(
            tool = %tool_name,
            error,
            "L4 auto-compact applied"
        );
        return RecoveryAction::Recovered {
            message: msg,
            level: 4,
        };
    }

    // --- L5: Reset ---
    let reset_result = reset_coordinator.execute(
        ResetScope::Conversation,
        ctx,
        loop_detector,
        steering,
        recovery,
    );
    if reset_result.success {
        tracker.record_success(tool_name);
        tracing::info!(
            tool = %tool_name,
            error,
            description = %reset_result.description,
            "L5 reset applied"
        );
        return RecoveryAction::Recovered {
            message: reset_result.description,
            level: 5,
        };
    }

    tracing::error!(
        tool = %tool_name,
        error,
        description = %reset_result.description,
        "L5 reset failed, entering fail-fast"
    );
    RecoveryAction::FailFast(format!(
        "L5 reset failed: {}",
        reset_result.description
    ))
}
