//! The [`ResetCoordinator`] — the L5 reset orchestrator.
//!
//! Tracks the cooldown window that follows a failed reset and
//! owns the logic that performs each `ResetScope`. The
//! coordinator itself does not hold the mutable state it
//! mutates (loop context, loop detector, steering channel,
//! recovery coordinator); callers pass those in to
//! [`ResetCoordinator::execute`].

use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use synthia_guardian::LoopDetectorSet;

use super::{result::ResetResult, scope::ResetScope};
use crate::{
    error_recovery::ErrorRecoveryCoordinator,
    loop_context::LoopContext,
    steering::SteeringChannel,
};

/// Cooldown duration applied after an L5 reset failure.
pub const RESET_COOLDOWN_SECS: u64 = 30;

/// Reset coordinator for L5 recovery.
pub struct ResetCoordinator {
    /// Deadline until which a new reset attempt must be refused.
    cooldown_until: Mutex<Option<Instant>>,
}

impl Default for ResetCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl ResetCoordinator {
    /// Creates a fresh coordinator with no active cooldown.
    pub fn new() -> Self {
        Self {
            cooldown_until: Mutex::new(None),
        }
    }

    /// Executes a reset of the given `scope`.
    ///
    /// For [`ResetScope::Conversation`]:
    /// - Discards `ctx.messages` and tool-result history.
    /// - Preserves `ctx.session_id` and the rest of session metadata.
    /// - Calls `LoopDetectorSet::reset()` to clear all loop detection.
    /// - Drains the steering channel (if provided).
    /// - Calls `recovery.record_success()` to clear the consecutive-error counter.
    ///
    /// If the coordinator is in cooldown, no reset is performed and a
    /// failed result is returned. On any failure, a 30s cooldown is
    /// started via [`ResetCoordinator::start_cooldown`].
    pub fn execute(
        &self,
        scope: ResetScope,
        ctx: &mut LoopContext,
        loop_detector: &mut LoopDetectorSet,
        steering: Option<&dyn SteeringChannel>,
        recovery: &ErrorRecoveryCoordinator,
    ) -> ResetResult {
        if let Some(remaining) = self.cooldown_remaining() {
            return ResetResult::failed(
                scope,
                format!(
                    "Reset refused: cooldown active ({}s remaining)",
                    remaining.as_secs()
                ),
            );
        }

        let result = match scope {
            ResetScope::Conversation => Self::execute_conversation_reset(
                ctx,
                loop_detector,
                steering,
                recovery,
            ),
            ResetScope::ToolState => {
                tracing::warn!(
                    "ToolState reset not implemented, falling back to Conversation"
                );
                Self::execute_conversation_reset(
                    ctx,
                    loop_detector,
                    steering,
                    recovery,
                )
            }
            ResetScope::Full => {
                tracing::warn!(
                    "Full reset not implemented, falling back to Conversation"
                );
                Self::execute_conversation_reset(
                    ctx,
                    loop_detector,
                    steering,
                    recovery,
                )
            }
        };

        if !result.success {
            self.start_cooldown();
        }
        result
    }

    /// Conversation-scope reset: discard `ctx.messages`, keep session
    /// metadata, clear loop detection, drain steering, reset error
    /// counter. HotMemory is owned outside `LoopContext` and is not
    /// touched here, which satisfies the spec requirement that it is
    /// preserved.
    fn execute_conversation_reset(
        ctx: &mut LoopContext,
        loop_detector: &mut LoopDetectorSet,
        steering: Option<&dyn SteeringChannel>,
        recovery: &ErrorRecoveryCoordinator,
    ) -> ResetResult {
        ctx.messages.clear();
        ctx.recent_tool_results.clear();
        ctx.cumulative_tokens = 0;
        ctx.needs_compact = false;
        // A fresh conversation starts at iteration 0; session_id and
        // span_ctx are preserved by design.
        ctx.iteration = 0;

        loop_detector.reset();

        if let Some(steering) = steering {
            steering.drain();
        }

        // Per spec: error counter is reset via record_success().
        recovery.record_success();

        ResetResult::success(
            ResetScope::Conversation,
            "Conversation reset: messages discarded, session preserved",
        )
    }

    /// Starts a 30s cooldown during which subsequent `execute()` calls
    /// will refuse to perform a reset.
    pub fn start_cooldown(&self) {
        let mut guard = self.cooldown_until.lock().unwrap();
        *guard =
            Some(Instant::now() + Duration::from_secs(RESET_COOLDOWN_SECS));
    }

    /// Clears any active cooldown. Primarily useful for tests.
    pub fn clear_cooldown(&self) {
        let mut guard = self.cooldown_until.lock().unwrap();
        *guard = None;
    }

    /// Returns true if a reset would currently be refused due to an
    /// active cooldown.
    pub fn is_in_cooldown(&self) -> bool {
        self.cooldown_remaining().is_some()
    }

    /// Returns the remaining cooldown duration, or `None` if no
    /// cooldown is active.
    pub fn cooldown_remaining(&self) -> Option<Duration> {
        let guard = self.cooldown_until.lock().unwrap();
        guard.and_then(|until| {
            let now = Instant::now();
            if now < until { Some(until - now) } else { None }
        })
    }

    /// Determines what scope of reset should be attempted based on error context.
    pub fn determine_scope(consecutive_errors: u64) -> ResetScope {
        match consecutive_errors {
            0..=5 => ResetScope::Conversation,
            6..=10 => ResetScope::ToolState,
            _ => ResetScope::Full,
        }
    }

    /// Validates whether a reset is safe to perform.
    pub fn is_safe_to_reset(has_unsaved_work: bool) -> bool {
        // Allow reset even with unsaved work at L5,
        // since we're in fail-fast territory anyway.
        !has_unsaved_work
    }

    /// Creates a reset result for a successful conversation reset.
    pub fn conversation_reset() -> ResetResult {
        ResetResult::success(
            ResetScope::Conversation,
            "Conversation history cleared",
        )
    }

    /// Creates a reset result for a failed reset.
    pub fn reset_failed(reason: impl Into<String>) -> ResetResult {
        ResetResult::failed(ResetScope::Full, reason)
    }
}
