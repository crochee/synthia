## MODIFIED Requirements

### Requirement: L1 Tool-Result Truncation

The agent loop MUST apply L1 truncation (`synthia_context::truncate::truncate_output`) to every tool result before injecting it into `ctx.messages`, regardless of `is_error`. Truncated results MUST include a "truncated" marker so the LLM can detect that content was clipped.

#### Scenario: Truncation triggered on oversized successful tool result
- **WHEN** a tool returns `output` longer than `TruncateConfig::default().max_bytes` and `is_error == false`
- **THEN** the truncated content is stored in `ctx.messages` (head + tail + marker)
- **AND** the agent loop yields `AgentEvent::System(SystemEvent::Recovery { level_number: 1, tool_name: Some(...), message: "Truncated tool output...", iteration })`

#### Scenario: Truncation triggered on oversized error tool result
- **WHEN** a tool returns `output` longer than `TruncateConfig::default().max_bytes` and `is_error == true`
- **THEN** the same truncation behavior applies (no special-casing for error results)
- **AND** `AgentEvent::System(SystemEvent::Recovery { level_number: 1, ... })` is still yielded

#### Scenario: Truncation not triggered on small tool result
- **WHEN** a tool returns `output` shorter than `TruncateConfig::default().max_bytes`
- **THEN** the original content passes through byte-identical
- **AND** no `Recovery` event is yielded for this tool result

---

### Requirement: L3-L5 Cascade Wired Into LLM Sampling Error Path

The agent loop MUST invoke `run_recovery_cascade` from `synthia_agent::error_recovery::recovery_cascade` when `StepSample::execute` returns `Err(e)`. The cascade MUST be invoked with the synthetic tool name `"llm_sample"` so per-tool fallback lookup is consistent.

#### Scenario: LLM sampling error triggers L3 fallback
- **WHEN** the LLM sampling fails with an error
- **AND** a fallback strategy is registered for `"llm_sample"` (or the LLM error has failed 2+ times)
- **THEN** `run_recovery_cascade` returns `RecoveryAction::Recovered(fallback_message)`
- **AND** the LLM error is NOT yielded as `SessionEnded`
- **AND** the agent loop yields `AgentEvent::System(SystemEvent::Recovery { level_number: 3, tool_name: Some("llm_sample"), message, iteration })`

#### Scenario: LLM sampling error escalates to L5 reset
- **WHEN** the LLM sampling fails repeatedly
- **THEN** the cascade escalates and yields `AgentEvent::System(SystemEvent::Recovery { level_number: 5, tool_name: Some("llm_sample"), message, iteration })`