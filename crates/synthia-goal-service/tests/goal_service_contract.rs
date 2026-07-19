//! Acceptance tests for PR-3.5 (`synthia-goal-service` scaffold).
//!
//! Acceptance criteria from `tasks.md` Task 4.1:
//!
//!   > `cargo check -p synthia-goal-service` exit code 0
//!
//! Plus five behavioural guarantees drawn from
//! `specs/goal-service-runtime/spec.md`:
//!
//! 1. The default impl is `CodeGoalService`.
//! 2. The `GoalService` trait is object-safe.
//! 3. The 7-state machine reaches terminal states.
//! 4. `TaskGoalHandle.subscribe()/update()` propagates state changes.
//! 5. Submitting then cancelling flows through `GoalError::UnknownGoal`.

#![allow(clippy::missing_const_for_fn)]

use std::sync::Arc;

use synthia_goal_service::{
    GoalError,
    GoalService,
    code::{CodeGoalService, default_service},
    task::{TaskGoal, TaskGoalId, TaskGoalState},
};

/// PR-3.5 acceptance: the default constructor returns a `CodeGoalService`
/// with the `GoalService` trait object view (object-safe).
#[test]
fn pr_3_5_default_impl_is_code_goal_service() {
    let svc = default_service();
    assert_eq!(svc.admitted(), 0);
    // Compile-time check: the trait is object-safe — assigning
    // `Arc<CodeGoalService>` to `Arc<dyn GoalService>` is a no-op for
    // our purposes here, but the call through the trait object must
    // succeed at runtime.
    let svc_typed: Arc<dyn GoalService> = default_service();
    assert_eq!(svc_typed.admitted(), 0);
}

/// PR-3.5 acceptance: the `GoalService` trait is object-safe and a
/// `CodeGoalService` is constructible with both the default and explicit
/// permit counts.
#[test]
fn pr_3_5_trait_object_safe() {
    let svc_default = CodeGoalService::new();
    assert!(svc_default.permits() >= 2);
    let svc_small = CodeGoalService::with_permits(1);
    assert_eq!(svc_small.permits(), 1);
    // Assignment to a trait object must compile.
    let _arc: Arc<dyn GoalService> = Arc::new(CodeGoalService::with_permits(3));
}

/// PR-3.5 acceptance: the 7-state lifecycle advances and refuses to
/// transition out of a terminal state.
#[test]
fn pr_3_5_seven_state_lifecycle_reaches_terminal() {
    let mut g = TaskGoal::new("acceptance-seven-state");
    assert_eq!(g.state, TaskGoalState::Pending);
    assert!(g.transition_to(TaskGoalState::Admitted));
    assert_eq!(g.state, TaskGoalState::Admitted);
    assert!(g.transition_to(TaskGoalState::Running));
    assert_eq!(g.state, TaskGoalState::Running);
    assert!(g.transition_to(TaskGoalState::Succeeded));
    assert!(g.state.is_terminal());
    // Cancelling a Succeeded goal must be a no-op.
    assert!(!g.transition_to(TaskGoalState::Cancelled));
}

/// PR-3.5 acceptance: a `TaskGoalHandle` propagates state changes through
/// its `watch` channel subscription.
#[tokio::test]
async fn pr_3_5_handle_propagates_state_changes() {
    use synthia_goal_service::task::TaskGoalHandle;

    let id = TaskGoalId::new();
    let handle = TaskGoalHandle::new(id, TaskGoalState::Pending);
    let mut rx = handle.subscribe();
    assert_eq!(*rx.borrow_and_update(), TaskGoalState::Pending);

    handle.update(TaskGoalState::Admitted);
    // The watch channel has changed; rx must observe the new value
    // before it returns from `changed()`.
    rx.changed().await.expect("state must propagate");
    assert_eq!(*rx.borrow(), TaskGoalState::Admitted);
}

/// PR-3.5 acceptance: cancelling an unknown id surfaces
/// `GoalError::UnknownGoal` carrying the id; cancelling a known id
/// succeeds and decreases `admitted()`.
#[tokio::test]
async fn pr_3_5_submit_then_cancel_flow() {
    let svc = CodeGoalService::new();
    let goal = TaskGoal::new("acceptance-cancel");
    let id = goal.id;

    // Submit: handler comes back, admitted count increments.
    let _handle = svc.submit(goal).await.expect("submit must succeed");
    assert_eq!(svc.admitted(), 1);

    // Cancel by id: must succeed.
    svc.cancel(id)
        .await
        .expect("cancel of a known id must succeed");
    assert_eq!(svc.admitted(), 0);

    // Cancelling the same id again: must surface `UnknownGoal`.
    let err = svc
        .cancel(id)
        .await
        .expect_err("re-cancel must surface UnknownGoal");
    assert!(matches!(err, GoalError::UnknownGoal(_)));
}
