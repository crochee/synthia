## ADDED Requirements

### Requirement: SessionRunCoordinator Run Arbitration
`SessionRunCoordinator` SHALL track per-session run state (`Idle`, `Running { run_id }`, `Interrupted { at }`). `run(key)` SHALL return `Err(AlreadyRunning)` if a run is active. `wake(key)` SHALL return the `RunId` of the woken run. `interrupt(key)` SHALL trip the run's cancellation token.

#### Scenario: Duplicate run rejected
- **WHEN** `run(key)` is called while the session already has a `Running` state
- **THEN** the coordinator SHALL return `Err(ServiceError::AlreadyRunning)`

#### Scenario: Wake sleeping session
- **WHEN** `wake(key)` is called on an `Idle` session
- **THEN** the coordinator SHALL return `Err(ServiceError::NoSuchRun)`

#### Scenario: Interrupt running session
- **WHEN** `interrupt(key)` is called on a `Running` session
- **THEN** the session's `OperationContext::cancellation` SHALL be triggered

---

### Requirement: RunGuard RAII Cleanup
`run(key)` SHALL return a `RunGuard` whose `Drop` impl transitions the session to `Idle`. `await_idle(key)` SHALL block until the session reaches `Idle`.

#### Scenario: RunGuard drop transitions to Idle
- **WHEN** a `RunGuard` is dropped
- **THEN** the session state SHALL transition to `Idle`
