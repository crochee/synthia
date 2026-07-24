//! Core data types + thresholds for the recovery cascade.
//!
//! The 3-variant [`RecoveryAction`] enum is the public
//! return type of [`super::run::run_recovery_cascade`];
//! callers pattern-match on it to decide what to do next
//! (re-inject a fallback / compact / reset marker, escalate,
//! or fail-fast).
//!
//! [`COMPACT_THRESHOLD`] is the one and only global
//! constant owned by the cascade; it's consumed by
//! [`super::l4::try_l4_compact`] to gate the L4
//! auto-compact decision. Lives here (not in `l4.rs`)
//! because both the orchestrator and the test suite
//! want to reference it by name.
//!
//! Kept separate from [`super::tracker`]
//! (the [`ConsecutiveFailureTracker`](super::tracker::ConsecutiveFailureTracker)
//! per-tool counter) and the three L*-step modules so
//! the type system + the constant stay grouped together.

/// Context utilization ratio above which L4 auto-compact is triggered.
/// Spec: `ctx.token_ratio() > 0.8` → auto-compact.
pub const COMPACT_THRESHOLD: f64 = 0.8;

/// Outcome of running the recovery cascade for a single tool failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryAction {
    /// Recovery succeeded. `message` is the marker to inject as the
    /// tool result (fallback guidance, compaction marker, or L5 reset
    /// marker). `level` records which cascade layer actually fired
    /// (3 = Fallback, 4 = Compact, 5 = Reset) so the caller can
    /// surface it on `AgentEvent::RecoveryApplied` without
    /// re-parsing the message string.
    Recovered { message: String, level: u32 },
    /// Recovery could not recover at the current level; caller should
    /// escalate to the next level. With L5 wired in this variant is
    /// retained for forward compatibility but is no longer produced by
    /// `run_recovery_cascade`.
    Escalate,
    /// Recovery exhausted; entering fail-fast mode.
    FailFast(String),
}
