//! `code` module — `CodeGoalService` implementation.
//!
//! PR-3.5 carried the type surface. PR-3.6 swaps the `parking_lot::Mutex`
//! for an `Arc<tokio::sync::Semaphore>` plus a `Weak<tokio::runtime::Handle>`
//! so that:
//!
//! - `submit` acquires a semaphore permit before admitting the goal;
//!   dropping the `TaskGoalHandle` releases the permit.
//! - A `Weak<Runtime>` reference allows the service to detect when the
//!   runtime has been dropped, surfacing
//!   [`GoalError::RuntimeUnavailable`](crate::GoalError::RuntimeUnavailable).

use std::{collections::HashMap, sync::Arc};

use parking_lot::Mutex;
use tokio::sync::OwnedSemaphorePermit;

use crate::{
    GoalError,
    GoalResult,
    GoalService,
    default_permits,
    task::{TaskGoal, TaskGoalHandle, TaskGoalId, TaskGoalState},
};

/// Default implementation of [`GoalService`].
///
/// Uses an `Arc<Semaphore>` for admission control and a
/// `Weak<tokio::runtime::Handle>` to detect runtime drop.
/// Dropping a goal's permit (via [`AdmittedGoal::drop`]) restores the
/// semaphore slot atomically.
pub struct CodeGoalService {
    /// Admission semaphore. Permits = capacity.
    semaphore: Arc<tokio::sync::Semaphore>,
    /// Weak reference to the tokio runtime. When `None` or upgraded to
    /// `None`, [`submit`](GoalService::submit) returns
    /// [`GoalError::RuntimeUnavailable`].
    runtime: Option<std::sync::Weak<tokio::runtime::Runtime>>,
    /// Tracked goals by id. Each entry holds an `AdmittedGoal` whose
    /// `Drop` impl releases the semaphore permit.
    goals: Mutex<HashMap<TaskGoalId, AdmittedGoal>>,
}

impl std::fmt::Debug for CodeGoalService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CodeGoalService")
            .field("permits", &self.semaphore.available_permits())
            .field("admitted", &self.admitted())
            .finish_non_exhaustive()
    }
}

/// An admitted goal holding a semaphore permit.
///
/// When dropped, the permit is released, freeing a slot in the
/// semaphore.
struct AdmittedGoal {
    /// Semaphore permit — released on drop.
    _permit: OwnedSemaphorePermit,
}

impl AdmittedGoal {
    /// Create a new admitted goal, taking ownership of the permit.
    fn new(permit: OwnedSemaphorePermit) -> Self {
        Self { _permit: permit }
    }
}

impl CodeGoalService {
    /// Construct a `CodeGoalService` with the default permit count.
    ///
    /// The default is `num_cpus * 2`, matching
    /// `specs/goal-service-runtime/spec.md`.
    #[must_use]
    pub fn new() -> Self {
        Self::with_permits(default_permits())
    }

    /// Construct a `CodeGoalService` with an explicit permit count.
    /// Values of `0` are coerced to `1` so `submit` never deadlocks.
    #[must_use]
    pub fn with_permits(permits: usize) -> Self {
        Self {
            semaphore: Arc::new(tokio::sync::Semaphore::new(permits.max(1))),
            runtime: None,
            goals: Mutex::new(HashMap::new()),
        }
    }

    /// Construct a `CodeGoalService` bound to a specific tokio runtime.
    ///
    /// The service holds a [`std::sync::Weak`] reference to the runtime;
    /// if the runtime is dropped, subsequent [`submit`](GoalService::submit)
    /// calls return [`GoalError::RuntimeUnavailable`].
    #[must_use]
    pub fn with_runtime(
        permits: usize,
        runtime: &Arc<tokio::runtime::Runtime>,
    ) -> Self {
        Self {
            semaphore: Arc::new(tokio::sync::Semaphore::new(permits.max(1))),
            runtime: Some(Arc::downgrade(runtime)),
            goals: Mutex::new(HashMap::new()),
        }
    }

    /// Number of permits advertised by the service (the capacity).
    #[must_use]
    pub fn permits(&self) -> usize {
        self.semaphore.available_permits() + self.admitted()
    }

    /// Whether the backing runtime is still alive.
    fn runtime_alive(&self) -> bool {
        self.runtime.as_ref().is_none_or(|w| w.upgrade().is_some())
    }
}

impl Default for CodeGoalService {
    fn default() -> Self {
        Self::new()
    }
}

/// Default factory used by tests + the public entry point called for in
/// `specs/goal-service-runtime/spec.md` ("Scenario: default impl is
/// `CodeGoalService`"). Returns an `Arc<dyn GoalService>` so callers
/// cannot accidentally rely on `CodeGoalService`-specific methods.
///
/// Note: the name is intentionally `default_service` (not `code`) to
/// avoid colliding with the `code` module that hosts `CodeGoalService`.
#[must_use]
pub fn default_service() -> Arc<dyn GoalService> {
    Arc::new(CodeGoalService::new())
}

#[async_trait::async_trait]
impl GoalService for CodeGoalService {
    async fn submit(&self, goal: TaskGoal) -> GoalResult<TaskGoalHandle> {
        // Check that the runtime is still alive (if bound).
        if !self.runtime_alive() {
            return Err(GoalError::RuntimeUnavailable);
        }

        // Acquire a semaphore permit (admission control).
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| GoalError::Sink("semaphore closed".into()))?;

        // Transition goal to Admitted.
        let mut goal = goal;
        goal.transition_to(TaskGoalState::Admitted);

        let handle = TaskGoalHandle::new(goal.id, TaskGoalState::Admitted);
        let admitted = AdmittedGoal::new(permit);

        self.goals.lock().insert(goal.id, admitted);

        Ok(handle)
    }

    async fn cancel(&self, id: TaskGoalId) -> GoalResult<()> {
        let removed = {
            let mut map = self.goals.lock();
            map.remove(&id)
        };
        // Dropping `removed` releases the semaphore permit.
        if removed.is_none() {
            return Err(GoalError::UnknownGoal(id));
        }
        Ok(())
    }

    fn admitted(&self) -> usize {
        self.goals.lock().len()
    }

    fn is_closed(&self) -> bool {
        !self.runtime_alive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_uses_default_permits() {
        let svc = CodeGoalService::new();
        assert!(svc.permits() >= 2);
        assert_eq!(svc.admitted(), 0);
    }

    #[test]
    fn with_permits_coerces_zero() {
        let svc = CodeGoalService::with_permits(0);
        assert_eq!(svc.permits(), 1);
    }

    #[test]
    fn default_service_factory_returns_object_safe_arc() {
        let svc = default_service();
        assert_eq!(svc.admitted(), 0);
    }

    #[tokio::test]
    async fn submit_acquires_permit_and_admits() {
        let svc = CodeGoalService::with_permits(2);
        let goal = TaskGoal::new("test-goal");
        let handle = svc.submit(goal).await.unwrap();
        assert_eq!(svc.admitted(), 1);
        assert_eq!(handle.initial_state, TaskGoalState::Admitted);
    }

    #[tokio::test]
    async fn cancel_releases_permit() {
        let svc = CodeGoalService::with_permits(2);
        let goal = TaskGoal::new("cancel-goal");
        let id = goal.id;
        svc.submit(goal).await.unwrap();
        assert_eq!(svc.admitted(), 1);

        svc.cancel(id).await.unwrap();
        assert_eq!(svc.admitted(), 0);
    }

    #[tokio::test]
    async fn admission_blocks_at_capacity() {
        let svc = CodeGoalService::with_permits(1);
        let g1 = TaskGoal::new("first");
        let _h1 = svc.submit(g1).await.unwrap();
        assert_eq!(svc.admitted(), 1);

        // Second submit should block (no permit available).
        // We verify that the available permits is 0.
        assert_eq!(svc.semaphore.available_permits(), 0);
    }

    #[test]
    fn runtime_drop_marks_closed() {
        // Cannot drop a tokio runtime inside an async context, so
        // we test with a non-async test and use block_on.
        let rt = Arc::new(tokio::runtime::Runtime::new().unwrap());
        let svc = CodeGoalService::with_runtime(2, &rt);
        assert!(!svc.is_closed());

        // Drop the runtime outside the async context.
        drop(rt);
        assert!(svc.is_closed());

        // Submit after drop should return RuntimeUnavailable.
        let goal = TaskGoal::new("after-drop");
        let err = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async { svc.submit(goal).await });
        assert!(matches!(err, Err(GoalError::RuntimeUnavailable)));
    }
}
