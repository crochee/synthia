## ADDED Requirements

### Requirement: Agent::run_stream SHALL auto-assemble tool orchestrator when not injected

When `Agent::run_stream` is invoked without an explicit `ToolOrchestrator`, the runtime MUST call `assemble_default_tool_orchestrator` internally to construct one. This ensures CLI/Examples callers do not silently lose sandbox/approval/retry capabilities. The auto-assembly MUST happen before the first tool call attempt and MUST log a warning when triggered (to aid debugging injection failures).

#### Scenario: CLI invokes run_stream without orchestrator

- **WHEN** `Agent::run_stream` is called with `tool_orchestrator: None`
- **THEN** the runtime calls `assemble_default_tool_orchestrator` to construct one
- **AND** a warning is logged with the message "auto-assembled tool orchestrator (caller did not inject one)"
- **AND** the assembled orchestrator provides sandbox/approval/retry equivalent to `Agent::resume`

#### Scenario: Explicit orchestrator injection is preserved

- **WHEN** `Agent::run_stream` is called with `tool_orchestrator: Some(orch)`
- **THEN** the runtime uses the injected orchestrator as-is
- **AND** no auto-assembly is performed
- **AND** no warning is logged

#### Scenario: Auto-assembly failure surfaces error

- **WHEN** `assemble_default_tool_orchestrator` returns an error
- **THEN** `run_stream` MUST propagate the error rather than silently continuing without an orchestrator
- **AND** the error message MUST contain "failed to assemble default tool orchestrator"

---

### Requirement: LoopContext SHALL be restored via from_metadata with all 4 fields

The main loop MUST restore `LoopContext` from `SessionMetadata` using `LoopContext::from_metadata(metadata)`. Manual partial restoration (e.g., restoring only `iteration` and `end_reason`) is prohibited. All 4 fields (`iteration`, `end_reason`, `cumulative_tokens`, `context_token_limit`) MUST be restored to preserve circuit_breaker accuracy.

#### Scenario: Resumed session restores iteration count

- **WHEN** a session is resumed after `metadata.iteration == 50` (with `max_iterations == 50`)
- **THEN** `LoopContext.iteration` is restored to 50
- **AND** the next loop iteration check immediately triggers `end_reason = MaxIterationsReached`
- **AND** no additional full iteration is executed

#### Scenario: Resumed session restores end_reason for doom_loop detection

- **WHEN** a session is resumed after `metadata.end_reason = Some(DoomLoopDetected)` with consecutive count = 2
- **THEN** `LoopContext.end_reason` is restored
- **AND** the doom_loop detector's consecutive count is preserved
- **AND** a third doom_loop trigger immediately reaches the threshold

#### Scenario: cumulative_tokens and context_token_limit are restored

- **WHEN** a session is resumed with `metadata.cumulative_tokens = 50000` and `metadata.context_token_limit = 100000`
- **THEN** both fields are restored into `LoopContext`
- **AND** subsequent token accounting continues from 50000 rather than 0
