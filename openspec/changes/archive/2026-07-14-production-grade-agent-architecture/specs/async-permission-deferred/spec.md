## ADDED Requirements

### Requirement: PermissionFuture SHALL provide awaitable permission result

`PermissionFuture` SHALL wrap a `tokio::sync::oneshot::Receiver<PermissionResult>` and implement the `Future` trait. The future SHALL resolve when the permission is granted, denied, or the sender is dropped.

#### Scenario: Future resolves on grant
- **WHEN** the permission sender calls `send(Ok(PermissionResult::Granted))`
- **THEN** awaiting the `PermissionFuture` SHALL return `Ok(PermissionResult::Granted)`

#### Scenario: Future resolves on deny
- **WHEN** the permission sender calls `send(Err(PermissionFutureError::Denied))`
- **THEN** awaiting the `PermissionFuture` SHALL return `Err(PermissionFutureError::Denied)`

#### Scenario: Future resolves on sender drop
- **WHEN** the permission sender is dropped without sending
- **THEN** awaiting the `PermissionFuture` SHALL return `Err(PermissionFutureError::Dropped)`

---

### Requirement: PermissionFuture SHALL support await_with_cancellation

`PermissionFuture` SHALL provide an `await_with_cancellation(token: &CancellationToken)` method that returns `Result<PermissionResult, PermissionFutureError>` and is cancelable via the passed `CancellationToken`.

#### Scenario: Await succeeds when permission granted before cancellation
- **WHEN** `await_with_cancellation(token)` is called
- **AND** permission is granted before the token is canceled
- **THEN** the method SHALL return `Ok(PermissionResult::Granted)`

#### Scenario: Await fails when token canceled before permission resolves
- **WHEN** `await_with_cancellation(token)` is called
- **AND** the token is canceled before permission is granted
- **THEN** the method SHALL return `Err(PermissionFutureError::Cancelled)`

#### Scenario: Immediate resolution for pre-resolved futures
- **WHEN** `PermissionFuture::immediate_granted()` or `PermissionFuture::immediate_denied()` is used
- **THEN** awaiting the future SHALL return immediately without suspending

---

### Requirement: PermissionService::ask SHALL return PermissionFuture non-blocking

The `PermissionService` trait SHALL provide an `ask(&self, request: PermissionRequest) -> PermissionFuture` method that returns immediately without blocking. The agent SHALL continue processing while waiting for the permission decision.

#### Scenario: Ask returns immediately without blocking
- **WHEN** `PermissionService::ask()` is called
- **THEN** the method SHALL return immediately with a `PermissionFuture`
- **AND** the agent SHALL continue processing without waiting

#### Scenario: Headless mode returns immediately denied future
- **WHEN** `HeadlessApprovalService::ask()` is called
- **THEN** the returned `PermissionFuture` SHALL immediately resolve to denied
- **AND** no user interaction is required

#### Scenario: TUI mode returns future that resolves on user action
- **WHEN** `TuiApprovalService::ask()` is called
- **THEN** a permission prompt SHALL be displayed to the user
- **AND** the returned `PermissionFuture` SHALL resolve when the user responds (Grant/Deny/Always)

---

### Requirement: Permission service SHALL support "remember always" persistence

When permission is granted with "always" option, the permission rule SHALL be persisted so future identical permission requests are auto-approved without user prompt.

#### Scenario: Always option persists rule
- **WHEN** user grants permission with "always" option
- **THEN** the permission rule SHALL be persisted to storage (database)
- **AND** future identical permission requests SHALL be auto-approved

#### Scenario: Persisted rule auto-approves
- **WHEN** a permission request matches a persisted "always" rule
- **AND** `PermissionService::ask()` is called
- **THEN** the returned `PermissionFuture` SHALL immediately resolve to granted
- **AND** no user prompt is displayed

---

### Requirement: DefaultToolOrchestrator SHALL await PermissionFuture

`DefaultToolOrchestrator` SHALL use the async `PermissionService::ask()` method instead of blocking `check()`. The orchestrator SHALL await the `PermissionFuture` before executing tools that require approval, with cancellation support.

#### Scenario: Orchestrator awaits permission before tool execution
- **WHEN** a tool call requires approval and `orchestrator.execute()` is called
- **THEN** `orchestrator.permission_service.ask(request)` SHALL be called
- **AND** the orchestrator SHALL await the returned `PermissionFuture`
- **AND** if denied, the tool SHALL fail with `ToolOrchestratorError::Denied`
- **AND** if granted, the tool SHALL proceed to execution

#### Scenario: Orchestrator cancels await on token
- **WHEN** orchestrator is awaiting permission
- **AND** the `CancellationToken` passed to `execute()` is canceled
- **THEN** the permission await SHALL be canceled
- **AND** the tool execution SHALL fail with `ToolOrchestratorError::Cancelled`
