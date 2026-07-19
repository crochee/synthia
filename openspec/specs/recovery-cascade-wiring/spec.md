## Purpose

Wire the already-implemented L1-L5 recovery cascade into the two actual error entry points in `stream_builder/builder.rs` (LLM sampling errors and tool execution errors) and emit `AgentEvent::RecoveryApplied` so external observers can see when recovery triggers. Without this, the cascade is "implemented dead code" — errors bypass recovery and the session terminates prematurely.

## Requirements

### Requirement: L1 Tool-Result Truncation

The agent loop MUST apply L1 truncation (`synthia_context::truncate::truncate_output`) to every tool result before injecting it into `ctx.messages`, regardless of `is_error`. Truncated results MUST include a "truncated" marker so the LLM can detect that content was clipped.

#### Scenario: Truncation triggered on oversized successful tool result
- **WHEN** a tool returns `output` longer than `TruncateConfig::default().max_bytes` and `is_error == false`
- **THEN** the truncated content is stored in `ctx.messages` (head + tail + marker)
- **AND** the agent loop yields `AgentEvent::RecoveryApplied { level_number: 1, tool_name: Some(...), message: "Truncated tool output...", iteration }`

#### Scenario: Truncation triggered on oversized error tool result
- **WHEN** a tool returns `output` longer than `TruncateConfig::default().max_bytes` and `is_error == true`
- **THEN** the same truncation behavior applies (no special-casing for error results)
- **AND** `AgentEvent::RecoveryApplied { level_number: 1, ... }` is still yielded

#### Scenario: Truncation not triggered on small tool result
- **WHEN** a tool returns `output` shorter than `TruncateConfig::default().max_bytes`
- **THEN** the original content passes through byte-identical
- **AND** no `AgentEvent::RecoveryApplied` is yielded for this tool result

---

### Requirement: L3-L5 Cascade Wired Into LLM Sampling Error Path

The agent loop MUST invoke `run_recovery_cascade` from `synthia_agent::error_recovery::recovery_cascade` when `StepSample::execute` returns `Err(e)`. The cascade MUST be invoked with the synthetic tool name `"llm_sample"` so per-tool fallback lookup is consistent.

#### Scenario: LLM sampling error triggers L3 fallback
- **WHEN** the LLM sampling fails with an error
- **AND** a fallback strategy is registered for `"llm_sample"` (or the LLM error has failed 2+ times)
- **THEN** `run_recovery_cascade` returns `RecoveryAction::Recovered(fallback_message)`
- **AND** the LLM error is NOT yielded as `AgentEvent::SessionEnded`
- **AND** the agent loop yields `AgentEvent::RecoveryApplied { level_number: 3, tool_name: Some("llm_sample"), message, iteration }`

#### Scenario: LLM sampling error escalates to L5 reset
- **WHEN** the LLM sampling fails repeatedly
- **AND** L3/L4 do not apply (no fallback, low context ratio)
- **THEN** L5 reset succeeds, `ctx.messages` is cleared, session continues
- **AND** `AgentEvent::RecoveryApplied { level_number: 5, ... }` is yielded

#### Scenario: LLM cascade exhausted yields SessionEnded
- **WHEN** `run_recovery_cascade` returns `RecoveryAction::FailFast(reason)`
- **THEN** the agent loop yields `AgentEvent::SessionEnded { reason: SessionEndReason::Error(reason) }` and returns
- **AND** no further iteration is attempted

---

### Requirement: L3-L5 Cascade Wired Into Tool Execution Error Path

The agent loop MUST invoke `run_recovery_cascade` when `StepToolExecute::execute` returns `Err(e)`. The cascade MUST be invoked with the actual `tool_name` of the failing call.

#### Scenario: Tool error triggers L3 fallback
- **WHEN** a tool execution returns `Err(e)`
- **AND** `FallbackProvider::get_fallback(tool_name)` returns `Some(strategy)` and the tool has failed 2+ times
- **THEN** `run_recovery_cascade` returns `RecoveryAction::Recovered(fallback_message)`
- **AND** the fallback message is injected as a `ToolResult { tool_name, output: fallback_message, is_error: true }` into `ctx.messages`
- **AND** the LLM receives the fallback guidance on the next iteration
- **AND** `AgentEvent::RecoveryApplied { level_number: 3, tool_name: Some(tool_name), message, iteration }` is yielded

#### Scenario: Tool error escalates to L4 auto-compact
- **WHEN** a tool execution returns `Err(e)`
- **AND** no fallback is registered
- **AND** `ctx.token_ratio() > 0.8`
- **THEN** L4 attempts `compact_with_fallback`
- **AND** on success, `ctx.messages` is replaced with compacted messages
- **AND** `AgentEvent::RecoveryApplied { level_number: 4, ... }` is yielded with the compaction marker

#### Scenario: Tool error escalates to L5 reset
- **WHEN** a tool execution returns `Err(e)`
- **AND** L3 and L4 do not apply
- **THEN** L5 reset executes `ResetScope::Conversation` successfully
- **AND** `ctx.messages` is cleared, `loop_detector` is reset, error counter is reset
- **AND** `AgentEvent::RecoveryApplied { level_number: 5, ... }` is yielded

---

### Requirement: RecoveryApplied Event Schema

The agent MUST emit `AgentEvent::RecoveryApplied` for every recovery action that takes effect (L1 truncation, L3 fallback, L4 compact, L5 reset). The event schema MUST be:

```rust
AgentEvent::RecoveryApplied {
    level_number: u32,       // 1 = Truncate, 2 = Retry, 3 = Fallback, 4 = Compact, 5 = Reset
    tool_name: Option<String>, // Some(name) for tool-specific recovery, None for LLM-only recovery
    message: String,          // human-readable description (e.g. "Context auto-compacted: 9000 -> 6000 tokens")
    iteration: usize,         // ctx.iteration when recovery fired
}
```

#### Scenario: Event emitted with correct level numbers
- **WHEN** a recovery action fires
- **THEN** the event's `level_number` MUST match the recovery level (1-5)
- **AND** the event's `message` MUST be non-empty

#### Scenario: Event includes tool_name for tool-specific recovery
- **WHEN** L1 truncation or L3-L5 cascade fires due to a tool error
- **THEN** the event's `tool_name` MUST be `Some(actual_tool_name)`

#### Scenario: Event tool_name is None for LLM-only recovery
- **WHEN** L1-L5 cascade fires due to an LLM sampling error
- **THEN** the event's `tool_name` MUST be `Some("llm_sample")` (synthetic tool name, not None)

---

### Requirement: BuilderSteps Carries Cascade State

`BuilderSteps` MUST own the mutable state required by `run_recovery_cascade`:
- `reset: ResetCoordinator` (cooldown window for failed resets)
- `failure_tracker: ConsecutiveFailureTracker` (per-tool consecutive failure counter)

The existing `recovery: ErrorRecoveryCoordinator` field MUST be retained.

#### Scenario: BuilderSteps constructs fresh state
- **WHEN** `BuilderSteps::new(config, hooks)` is called
- **THEN** `reset` is a fresh `ResetCoordinator::new()` (no cooldown)
- **AND** `failure_tracker` is a fresh `ConsecutiveFailureTracker::new()`
- **AND** `recovery` is a fresh `ErrorRecoveryCoordinator::new(5)` (5s cooldown, unchanged)

#### Scenario: L5 reset success clears failure tracker
- **WHEN** `ResetCoordinator::execute(ResetScope::Conversation, ...)` succeeds
- **THEN** `failure_tracker` is cleared (per cascade implementation)

#### Scenario: L5 reset cooldown observed on subsequent attempts
- **WHEN** `ResetCoordinator::execute` fails and starts a 30s cooldown
- **THEN** subsequent `run_recovery_cascade` calls within 30s return `RecoveryAction::FailFast`

---

### Requirement: Recovery Coordination Does Not Mutate Error Result Semantics

Wiring up the cascade MUST NOT change the `RecoveryResult` enum (`Recovered | Escalated | FailFast`) or the `RecoveryAction` enum (`Recovered | Escalate | FailFast`). The existing `error_recovery/*` module public API MUST remain stable so that archive specs (`auto-compact-on-error`, `session-reset`, `tool-fallback`, `tool-output-truncate`, `tool-retry`) continue to validate.

#### Scenario: Error_recovery module public API unchanged
- **WHEN** this change is applied
- **THEN** the following APIs have **identical signatures and behavior** as in commit `59eb0e1` (error-recovery-cascade base):
  - `ErrorRecoveryCoordinator::new`, `handle_error`, `record_success`, `calculate_backoff`
  - `run_recovery_cascade`
  - `ResetCoordinator::new`, `execute`, `start_cooldown`, `clear_cooldown`
  - `FallbackProvider::get_fallback`, `has_fallback`
  - `ConsecutiveFailureTracker` methods

#### Scenario: Archive specs continue to validate
- **WHEN** `openspec validate` is run after this change
- **THEN** the 5 archive specs (`auto-compact-on-error`, `session-reset`, `tool-fallback`, `tool-output-truncate`, `tool-retry`) continue to pass

---

### Requirement: Cascade Is Not Invoked for Successful Operations

The agent loop MUST NOT invoke `run_recovery_cascade` or yield `AgentEvent::RecoveryApplied` for successful tool executions or successful LLM calls.

#### Scenario: Successful tool execution does not trigger recovery
- **WHEN** `StepToolExecute::execute` returns `Ok(results)`
- **THEN** no cascade call is made
- **AND** no `AgentEvent::RecoveryApplied` is yielded (unless L1 truncate was triggered on a large result)

#### Scenario: Successful LLM sampling does not trigger recovery
- **WHEN** `StepSample::execute` returns `Ok(sampling_result)`
- **THEN** no cascade call is made
- **AND** no `AgentEvent::RecoveryApplied` is yielded
