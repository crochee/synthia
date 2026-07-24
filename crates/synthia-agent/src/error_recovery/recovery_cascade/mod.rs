//! Recovery cascade coordinator: L3 Fallback → L4 Auto-Compact → L5 Reset.
//!
//! L1 (truncate) and L2 (retry) are handled inline at the tool-execution
//! boundary. This module implements the higher-level cascade that is
//! invoked when a tool call ultimately still fails after L1/L2 have run.
//!
//! See `openspec/changes/error-recovery-cascade/specs/auto-compact-on-error`
//! for the L4 contract and `specs/session-reset` for the L5 contract.
//!
//! The original 702-line `recovery_cascade.rs` was split into focused
//! submodules by responsibility:
//!
//! - `core`: the [`RecoveryAction`] 3-variant enum +
//!   the [`COMPACT_THRESHOLD`] constant.
//! - `tracker`: the [`ConsecutiveFailureTracker`]
//!   per-tool failure counter (5 methods).
//! - `l3`: the L3 fallback step ([`l3::try_l3_fallback`]).
//! - `l4`: the L4 auto-compact step ([`l4::try_l4_compact`]).
//! - `run`: the L3 → L4 → L5 orchestrator
//!   ([`run::run_recovery_cascade`]).
//!
//! The 12 unit tests live in `tests`.

mod core;
mod l3;
mod l4;
mod run;
mod tracker;

#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests;

pub use core::{COMPACT_THRESHOLD, RecoveryAction};

pub use run::run_recovery_cascade;
pub use tracker::ConsecutiveFailureTracker;
