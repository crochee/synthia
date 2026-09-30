//! Process-wide usage counters for `/api/v1/chat/usage`.

use serde::Serialize;

/// Process-wide usage counters. Cheap to read (lock-free
/// `std::sync::atomic` snapshot) so the `/api/v1/chat/usage`
/// endpoint can poll without contention.
#[derive(Debug, Default)]
pub struct UsageMetrics {
    pub tokens_in: std::sync::atomic::AtomicU64,
    pub tokens_out: std::sync::atomic::AtomicU64,
    /// Finished runs, counted on the terminal
    /// `SystemEvent::SessionEnded` **whatever its
    /// [`SessionEndReason`](synthia::harness::SessionEndReason)** — a
    /// cancelled, errored or interrupted run still consumed a turn,
    /// so counting only `Completed` would under-report the work the
    /// process actually did.
    ///
    /// Written by the session controllers' event funnel (one
    /// increment per `SessionEnded` per run); read-only here.
    pub turns: std::sync::atomic::AtomicU64,
}

impl UsageMetrics {
    /// Atomic snapshot of the three counters. Field order in
    /// `UsageResponse` mirrors this struct's order — `tokens_in`
    /// before `tokens_out` before `turns`.
    pub fn snapshot(&self) -> UsageSnapshot {
        UsageSnapshot {
            tokens_in: self
                .tokens_in
                .load(std::sync::atomic::Ordering::Relaxed),
            tokens_out: self
                .tokens_out
                .load(std::sync::atomic::Ordering::Relaxed),
            turns: self.turns.load(std::sync::atomic::Ordering::Relaxed),
        }
    }
}

/// Plain-data snapshot of [`UsageMetrics`] for the API
/// response. Cheap to clone via `Copy` since the underlying
/// counters are already integers.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct UsageSnapshot {
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub turns: u64,
}
