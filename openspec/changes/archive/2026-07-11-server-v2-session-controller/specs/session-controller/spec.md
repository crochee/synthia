## ADDED Requirements

### Requirement: Single run guarantee
The `SessionController` SHALL ensure that at most one `Agent::run_stream` is active for a given session at any time.

#### Scenario: Concurrent prompts do not spawn multiple runs
- **WHEN** two clients send `POST /api/v2/sessions/{id}/prompts` while the session is already `Running`
- **THEN** the second prompt MUST be appended to `session_input.jsonl` and consumed by the existing run's steering drain, and a second `Agent::run_stream` MUST NOT be spawned

### Requirement: Prompt admission
The `SessionController` SHALL append admitted prompts to the session input queue and wake the run loop.

#### Scenario: Prompt while Idle
- **WHEN** a prompt arrives while the session is `Idle`
- **THEN** the controller MUST transition to `Running`, spawn `Agent::run_stream`, and consume the prompt

#### Scenario: Prompt while Running
- **WHEN** a prompt arrives while the session is `Running`
- **THEN** the controller MUST append it to the queue without spawning a new run

### Requirement: Steering admission
The `SessionController` SHALL treat steering messages as high-priority inputs.

#### Scenario: Steering while Running
- **WHEN** a steering message arrives while the session is `Running`
- **THEN** the controller MUST append it to `session_input.jsonl` with priority `255` so it is consumed before regular queued prompts

### Requirement: Cancel propagation
The `SessionController` SHALL propagate cancel requests to the active run's `CancellationToken`.

#### Scenario: Cancel during tool execution
- **WHEN** a cancel request arrives while a tool is executing
- **THEN** the controller MUST call `cancel_token.cancel()`, the active run MUST terminate, and the state MUST transition to `Cancelled` and then back to `Idle`

### Requirement: Controller lifecycle
The server SHALL create a `SessionController` on first access and shut it down after idle timeout.

#### Scenario: Controller created on first prompt
- **WHEN** the first `POST /api/v2/sessions/{id}/prompts` arrives
- **THEN** the server MUST create a `SessionController` for that session if one does not exist

#### Scenario: Controller shutdown after idle
- **WHEN** a controller has no active run and no SSE subscribers for the configured idle timeout
- **THEN** the server MUST send `SessionOp::Shutdown`, drop the controller, and remove it from the in-memory map
