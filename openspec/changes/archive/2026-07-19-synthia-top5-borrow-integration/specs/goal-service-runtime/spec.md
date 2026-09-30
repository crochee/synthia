# Capability: goal-service-runtime

> **Status**: Proposed (change #1: 架构基础设施)
> **Source**: codex `codex-core/src/state/goal_service.rs` (~420 行)

## Purpose

将现有 `synthia-service::goal` (190 行 stub) 替换为独立 `synthia-goal-service` crate，提供 `CodeGoalService` via `Arc<tokio::sync::Semaphore>` admission control、`Weak` runtime + idle eviction、Keep/Set OCC retry。

## ADDED Requirements

### Requirement: GoalService trait + CodeGoalService impl

The `synthia-goal-service` crate MUST expose a `GoalService` trait and ship `CodeGoalService` as the only production implementation.

#### Scenario: default impl is CodeGoalService

- **WHEN** a consumer constructs a default `GoalService` via `GoalService::code()`
- **THEN** the returned value MUST be `Arc<CodeGoalService>` wrapping `Arc<tokio::sync::Semaphore>` + `Weak<Runtime>`
- **AND** MUST accept a configurable `SemaphorePermits` bound (default = num_cpus * 2)

#### Scenario: trait swap rejection

- **WHEN** a consumer writes `let svc: Arc<dyn GoalService> = GoalService::code()`
- **THEN** the build MUST succeed (trait object safe)
- **AND** the trait MUST NOT have generic methods requiring monomorphization

### Requirement: semaphore-based admission control

The `CodeGoalService` MUST use a `Semaphore` to cap concurrent goals per runtime.

#### Scenario: admission on capacity

- **WHEN** `submit(goal)` is called and permits are available
- **THEN** the call MUST return immediately with a `GoalHandle`
- **AND** MUST decrement available permits by 1

#### Scenario: admission blocked

- **WHEN** `submit(goal)` is called and no permits are available
- **THEN** the call MUST await on the semaphore (non-polling)
- **AND** MUST be cancelled if the runtime drops (per Weak runtime)

### Requirement: Weak runtime + idle eviction

The `CodeGoalService` MUST hold a `Weak<tokio::runtime::Handle>` and MUST evict idle goal slots when the runtime drops.

#### Scenario: weak runtime check

- **WHEN** the runtime is still alive
- **THEN** `submit` MUST spawn onto the runtime
- **AND** the goal MUST execute

#### Scenario: runtime dropped mid-flight

- **WHEN** the runtime drops while a goal is awaiting permit
- **THEN** the await MUST return `GoalError::RuntimeUnavailable`
- **AND** the goal MUST NOT leak permits

### Requirement: Keep/Set OCC retry

The `CodeGoalService` MUST implement optimistic concurrency control with `Keep` (writer) and `Set` (state setter) operations, retrying up to 3 times on conflict.

#### Scenario: OCC retry

- **WHEN** two clients attempt a conflicting Keep + Set within 10ms
- **THEN** the second MUST observe a `Conflict` and MUST retry automatically up to 3 times
- **AND** MUST return `GoalError::MaxRetriesExceeded` after the 3rd failure

#### Scenario: OCC success

- **WHEN** no concurrent operations occur
- **THEN** Keep MUST succeed and Set MUST commit the new state under OCC version `n+1`
