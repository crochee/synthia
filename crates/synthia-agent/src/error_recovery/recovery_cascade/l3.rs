//! L3 fallback step.
//!
//! [`try_l3_fallback`] is the smallest of the three
//! cascade helpers: it increments the per-tool failure
//! counter (via
//! [`super::tracker::ConsecutiveFailureTracker::record_failure`])
//! and, if the tool has a registered fallback strategy
//! in [`crate::error_recovery::fallback::FallbackProvider`]
//! AND the failure count has reached 2+, returns the
//! fallback message; otherwise `None`.
//!
//! The increment happens *before* the lookup so the
//! tracker reflects every tool failure (with or without
//! a registered fallback) — important for tests that
//! assert on the post-call counter value.
//!
//! Kept separate from [`super::l4`] (the L4 auto-compact
//! step) and [`super::run`] (the orchestrator) so the
//! three L*-step concerns can evolve independently.

use super::tracker::ConsecutiveFailureTracker;
use crate::error_recovery::fallback::FallbackProvider;

/// L3: if the tool has a registered fallback strategy and it has failed
/// 2+ times, return the fallback message; otherwise `None`.
///
/// The per-tool failure counter is incremented before the fallback lookup
/// so that the tracker reflects every tool failure (with or without a
/// registered fallback).
pub(crate) fn try_l3_fallback(
    tool_name: &str,
    tracker: &mut ConsecutiveFailureTracker,
) -> Option<String> {
    let failures = tracker.record_failure(tool_name);
    let strategy = FallbackProvider::get_fallback(tool_name)?;
    if failures >= 2 {
        Some(strategy.action)
    } else {
        None
    }
}
