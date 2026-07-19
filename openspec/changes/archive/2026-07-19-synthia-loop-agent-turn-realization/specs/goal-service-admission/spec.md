# Spec: goal-service-admission

## ADDED Requirements

### Requirement: GoalService fields in LoopServices

`LoopServices` SHALL contain:
- `goal_service: Arc<dyn GoalService>` — for admission control (submit/cancel)
- `goal_tracker: Arc<dyn goal::GoalService>` — for progress tracking (current/set/status)

Both fields SHALL be optional (`Option<Arc<...>>`) to support configurations without goal services.

#### Scenario: LoopServices with GoalService configured

WHEN `LoopServices` is constructed with `goal_service: Some(arc_service)`
THEN `loop_services.goal_service` SHALL return `Some(&Arc<dyn GoalService>)`

#### Scenario: LoopServices without GoalService

WHEN `LoopServices` is constructed with `goal_service: None`
THEN main_loop SHALL skip the admission gate and proceed directly

### Requirement: Admission gate at turn start

The main_loop SHALL call `GoalService::submit()` at the beginning of each turn (before `IterationStarted` event). If the submission fails (capacity reached), the turn SHALL wait until a slot is available or the cancellation token is triggered.

#### Scenario: Admission granted

WHEN `GoalService::submit(task_goal)` returns `Ok(TaskGoalHandle)`
THEN the main_loop SHALL proceed with the turn
AND the `TaskGoalHandle` SHALL be stored in `LoopContext` for the turn's duration

#### Scenario: Admission at capacity

WHEN `GoalService::submit(task_goal)` returns an error indicating the semaphore is full
THEN the main_loop SHALL yield a `TokenBudgetWarning` event
AND wait for a slot to become available (listen on `TaskGoalHandle` state changes)
OR until the cancellation token is triggered

#### Scenario: Admission cancelled

WHEN the cancellation token is triggered while waiting for admission
THEN the main_loop SHALL call `GoalService::cancel(goal_id)` if a handle was obtained
AND exit the loop with `SessionEndReason::Cancelled`

### Requirement: Goal tracking integration

After tool execution and LLM response, the main_loop SHALL call `goal_tracker` methods to report progress.

#### Scenario: Report tool execution to goal tracker

WHEN a tool execution completes successfully
THEN main_loop SHALL call `goal_tracker.set(GoalUpdate::ToolCompleted { tool_id, result_summary })`

#### Scenario: Report LLM response to goal tracker

WHEN an LLM response is received
THEN main_loop SHALL call `goal_tracker.set(GoalUpdate::LlmResponseReceived { token_count })`
