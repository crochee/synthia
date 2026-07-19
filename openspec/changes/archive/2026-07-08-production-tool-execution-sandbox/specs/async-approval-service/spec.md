## ADDED Requirements

### Requirement: ApprovalService SHALL provide an asynchronous approval request interface
The system SHALL define an `ApprovalService` trait with an async method `request_approval` that waits for a human or Guardian decision on a pending tool invocation.

#### Scenario: CLI session requests approval
- **WHEN** a tool invocation requires confirmation in a CLI session
- **THEN** the CLI implementation of `ApprovalService` SHALL prompt the user and suspend the agent loop until a decision is returned.

#### Scenario: Server session requests approval
- **WHEN** a tool invocation requires confirmation in a server session
- **THEN** the server implementation of `ApprovalService` SHALL send an approval request over the active transport and await the client response.

---

### Requirement: ApprovalService SHALL default to deny on timeout, cancellation, or service unavailability
If `request_approval` does not receive a decision within the configured timeout, or the waiting task is cancelled, or the approval service is unavailable, the system SHALL treat the outcome as `Deny`.

#### Scenario: Approval timeout
- **WHEN** five minutes elapse without a user response
- **THEN** `ApprovalService` SHALL return `ApprovalOutcome::Deny`.

#### Scenario: Session disconnect
- **WHEN** the client disconnects while an approval request is pending
- **THEN** `ApprovalService` SHALL cancel the pending request and return `ApprovalOutcome::Deny`.

---

### Requirement: ApprovalStore SHALL cache decisions for the lifetime of a session
The system SHALL provide an `ApprovalStore` that caches `once`, `always-for-session`, and `reject` decisions, keyed by a deterministic scope derived from tool name and arguments.

#### Scenario: User approves always for session
- **WHEN** a user responds "always for this session" to an approval prompt
- **THEN** `ApprovalStore` SHALL cache the decision and SHALL skip future approval requests for matching invocations within the same session.

#### Scenario: User rejects once
- **WHEN** a user responds "no" to an approval prompt
- **THEN** `ApprovalStore` SHALL cache the rejection and SHALL immediately deny subsequent matching invocations without re-prompting.

---

### Requirement: ApprovalStore SHALL NOT persist "always" decisions across sessions by default
Session-scoped approvals SHALL remain in memory and SHALL be discarded when the session ends, unless explicitly exported to a user-level rules file.

#### Scenario: Session restart
- **WHEN** a session is resumed from disk after shutdown
- **THEN** previous session-scoped approvals SHALL NOT be automatically restored.

---

### Requirement: ApprovalService SHALL support a headless deny fallback
When running in an environment without an interactive UI, the default `ApprovalService` implementation SHALL return `Deny` for all `RequireConfirm` requests.

#### Scenario: CI headless run
- **WHEN** Synthia runs in a CI pipeline with no terminal attached
- **THEN** `ApprovalService` SHALL deny confirmation requests and the tool invocation SHALL fail gracefully.
