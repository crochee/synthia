//! Reset strategy for L5 recovery.
//!
//! Handles agent state reset when error recovery reaches L5.
//!
//! See `openspec/changes/error-recovery-cascade/specs/session-reset` for
//! the L5 contract: when L4 compact fails, the system performs a
//! `ResetScope::Conversation` reset which discards `ctx.messages` and
//! recent tool results, preserves session ID and HotMemory, resets the
//! consecutive-error counter, clears loop detection state, and drains
//! any pending steering messages.
//!
//! # Module Layout
//!
//! - [`scope`]: [`scope::ResetScope`] enum (3 variants:
//!   `Conversation` / `ToolState` / `Full`).
//! - [`result`]: [`result::ResetResult`] struct + 2
//!   constructors (`success` / `failed`).
//! - [`coordinator`]: [`coordinator::ResetCoordinator`] struct +
//!   the `RESET_COOLDOWN_SECS` const + its 10 methods
//!   (4 lifecycle / cooldown / 4 scope logic / 2 builder).
//! - [`deadlock`]: [`deadlock::DeadlockPrevention`] struct
//!   + 4 methods (timeout / is_deadlocked /
//!     `default_threshold_secs`).
//! - [`tests`]: 15 unit tests covering scope-based decision,
//!   reset-result builders, deadlock detection (4),
//!   conversation reset (3 — discards messages / clears
//!   loop detector / drains steering), cooldown (3 —
//!   starts on failure / refuses during cooldown /
//!   cleared by `clear_cooldown`).

mod coordinator;
mod deadlock;
mod result;
mod scope;

#[cfg(test)]
mod tests;

pub use coordinator::{RESET_COOLDOWN_SECS, ResetCoordinator};
pub use deadlock::DeadlockPrevention;
pub use result::ResetResult;
pub use scope::ResetScope;
