//! `synthia-goal-service` — `CodeGoalService` for Synthia.
//!
//! ## Phase A scaffold (PR-3.5, change #1)
//!
//! This file is intentionally minimal: it exposes the trait surface and
//! the `TaskGoal` 7-state placeholder, so subsequent PRs (`3.6`, `3.7`)
//! fill in the admission control and OCC retry.
//!
//! - PR-3.5 (this file): `GoalService` trait + `TaskGoal` 7-state struct
//!   + `GoalError` enum + module paths.
//! - PR-3.6: `CodeGoalService` via `Arc<tokio::sync::Semaphore>` + Weak
//!   runtime.
//! - PR-3.7: `Keep`/`Set` OCC retry + eviction.
//!
//! See `specs/goal-service-runtime/spec.md` for the normative
//! requirements.

#![allow(clippy::module_inception)] // `code` module holds the `CodeGoalService` impl.

pub mod code;
pub mod occ;
pub mod task;

/// Error variants surfaced from any [`GoalService`] implementation.
#[derive(Debug, thiserror::Error)]
pub enum GoalError {
    /// The runtime backing the service has been dropped.
    #[error("goal runtime unavailable")]
    RuntimeUnavailable,
    /// All retry slots for an OCC conflict have been exhausted.
    #[error("goal OCC retry exhausted after {attempts} attempts")]
    MaxRetriesExceeded {
        /// Number of attempts the call made before giving up.
        attempts: u8,
    },
    /// The supplied goal id does not exist in the active registry.
    #[error("goal id {0} not found")]
    UnknownGoal(task::TaskGoalId),
    /// Catch-all for sink-side errors that downstream PRs will narrow.
    #[error("goal service error: {0}")]
    Sink(String),
}

/// Result alias used by all [`GoalService`] methods.
pub type GoalResult<T> = Result<T, GoalError>;

/// Default admission permits per runtime.
///
/// Matches the value called out in `specs/goal-service-runtime/spec.md`
/// ("default = `num_cpus * 2`"). Synthia's project-wide clippy policy
/// bans `Result::map_or` and prefers `.map(..).unwrap_or(..)` for
/// legibility; we allow `clippy::map_unwrap_or` at this single call
/// site so both policies are satisfied.
#[must_use]
#[allow(clippy::map_unwrap_or)]
pub fn default_permits() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get() * 2)
        .unwrap_or(2)
}

/// The trait surface every goal-service implementation must satisfy.
///
/// The trait is object-safe: every method takes `&self`, returns a
/// concrete type, and uses no generics on the method signature.
#[async_trait::async_trait]
pub trait GoalService: Send + Sync + 'static {
    /// Submit a new goal.
    ///
    /// `permits` is computed by the caller (typically via
    /// [`default_permits`]); the implementation may apply its own cap.
    async fn submit(
        &self,
        goal: task::TaskGoal,
    ) -> GoalResult<task::TaskGoalHandle>;

    /// Cancel a previously-submitted goal by id.
    async fn cancel(&self, id: task::TaskGoalId) -> GoalResult<()>;

    /// Current number of admitted (i.e. capacity-consuming) goals.
    fn admitted(&self) -> usize;

    /// Whether the service has been closed / shut down.
    fn is_closed(&self) -> bool {
        false
    }
}
