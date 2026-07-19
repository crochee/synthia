<!--
Delta spec for new capability: tool-interrupt-cleanup
-->

## ADDED Requirements

### Requirement: ToolOrchestrator SHALL expose fail_interrupted_tools for batch cleanup

The `DefaultToolOrchestrator` SHALL expose a `fail_interrupted_tools(&self) -> usize` method that iterates over all entries in `active_calls: DashMap<String, CancellationToken>`. For each entry, the method SHALL: (1) cancel the entry's `CancellationToken`, (2) remove the entry from the map, (3) emit a `ToolCallCompleted` event with `is_error: true` and output `"Tool execution interrupted"`. The method SHALL return the number of tools that were interrupted.

#### Scenario: Interrupt with multiple active tools
- **WHEN** `fail_interrupted_tools()` is called with 3 active tool calls
- **THEN** all 3 tools SHALL be canceled
- **AND** 3 `ToolCallCompleted` events with `is_error: true` SHALL be emitted
- **AND** `active_calls` SHALL be empty
- **AND** the return value SHALL be 3

#### Scenario: Interrupt with no active tools
- **WHEN** `fail_interrupted_tools()` is called with an empty `active_calls`
- **THEN** no events SHALL be emitted
- **AND** the return value SHALL be 0
- **AND** `active_calls` SHALL remain empty

#### Scenario: Concurrent tool completion during interrupt
- **WHEN** `fail_interrupted_tools()` is iterating and a tool concurrently completes (removes itself from `active_calls`)
- **THEN** the concurrently-completed tool SHALL be skipped without panic
- **AND** remaining tools SHALL still be interrupted

---

### Requirement: Agent main loop SHALL call fail_interrupted_tools on any interruption

The agent main loop SHALL call `tool_orchestrator.fail_interrupted_tools()` whenever an interruption is detected, including: (1) `cancel_token.cancelled()`, (2) steering interruption, (3) session abort. This ensures no zombie tool calls remain in `active_calls` after interruption.

#### Scenario: Cancellation token triggered
- **WHEN** the agent's `cancel_token` is canceled during tool execution
- **THEN** `fail_interrupted_tools()` SHALL be called
- **AND** all active tools SHALL receive `ToolCallCompleted { is_error: true }` events

#### Scenario: Steering interruption during tool execution
- **WHEN** a steering message arrives during tool execution
- **THEN** after the current tool completes, `fail_interrupted_tools()` SHALL be called for remaining active tools
- **AND** the steering message SHALL be processed in the next iteration

---

### Requirement: Interrupted tool events SHALL be visible to LLM and persisted

The `ToolCallCompleted` events emitted by `fail_interrupted_tools` SHALL be routed through the same event pipeline as normal tool completions. The events SHALL be: (1) persisted to the session event log, (2) added to `ctx.recent_tool_results` for LLM visibility, (3) mirrored to parent session if applicable.

#### Scenario: LLM sees interrupted tool results
- **WHEN** `fail_interrupted_tools()` emits events for 2 interrupted tools
- **THEN** both events SHALL appear in `ctx.recent_tool_results`
- **AND** the LLM SHALL see "Tool execution interrupted" in the next turn

#### Scenario: Interrupted events persisted to event log
- **WHEN** `fail_interrupted_tools()` emits events
- **THEN** the events SHALL be appended to the session JSONL with `is_error: true`
- **AND** replay SHALL reproduce the interrupted state
