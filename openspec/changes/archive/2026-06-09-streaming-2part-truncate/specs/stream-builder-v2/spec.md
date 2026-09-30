# stream-builder-v2 Specification (delta)

## Purpose

This is a delta spec for the existing `stream-builder-v2` capability. The full specification is at `openspec/specs/stream-builder-v2/spec.md`. This file documents the requirements being modified by the `streaming-2part-truncate` change.

## MODIFIED Requirements

### Requirement: StepExecutor shall execute each phase of the agent loop

StepExecutor SHALL provide `step_sample()`, `step_tool_execute()`, `step_compact_check()` methods that execute each phase of the agent loop in order. `step_sample()` SHALL use `ModelProvider::complete_with_stream` (callback-based) and SHALL NOT use the deprecated `ModelProvider::stream()` method.

#### Scenario: step_sample executes LLM request via complete_with_stream
- **WHEN** `step_sample()` is called with a `LoopContext`
- **THEN** it SHALL call `provider.complete_with_stream(req, on_delta)` (NOT `provider.stream(req)`)
- **THEN** it SHALL yield `LlmRequestStarted` and `LlmStreamDelta` events
- **THEN** it SHALL yield one `LlmStreamDelta` per `StreamChunk::Content(ContentPart::Text)` received
- **THEN** it SHALL yield one `ToolCallDelta` event per `StreamChunk::ToolCallDelta` received
- **THEN** it SHALL return a `SamplingResult` extracted from `StreamChunk::IsDone { result }` (or from a `complete()` fallback if the stream closed early)

#### Scenario: step_sample handles stream fallback
- **WHEN** `complete_with_stream` returns a stream that closes without emitting `IsDone`
- **THEN** `step_sample()` SHALL log a `stream_closed_early` warning
- **THEN** it SHALL increment the `stream_closed_early_total` metric
- **THEN** it SHALL retry the same request via `provider.complete(req)` exactly once
- **THEN** it SHALL return the `SamplingResult` from the fallback `complete()` call

#### Scenario: step_sample respects CancellationToken
- **WHEN** the `CancellationToken` is cancelled while `complete_with_stream` is in progress
- **THEN** `step_sample()` SHALL drop the receiver side of the bounded channel
- **THEN** the provider task SHALL observe channel closure and cancel the upstream HTTP request within 5 seconds
- **THEN** `step_sample()` SHALL return `Err(AgentError::Cancelled)`

#### Scenario: step_tool_execute runs tools
- **WHEN** `step_tool_execute()` is called with `tool_calls`
- **THEN** it SHALL execute each tool via `tool_registry`
- **THEN** it SHALL yield `ToolCallStarted` and `ToolCallCompleted` events
- **THEN** it SHALL return `ToolResults` for each tool call
- **THEN** any tool result whose content exceeds 30,000 bytes SHALL be passed through `synthia_context::truncate::truncate_output` before being returned for context assembly

#### Scenario: step_compact_check evaluates token budget
- **WHEN** `step_compact_check()` is called after any message addition
- **THEN** it SHALL check if token count exceeds budget thresholds
- **THEN** it SHALL trigger compaction if `MustCompact` status is detected
- **THEN** it SHALL yield `TokenBudgetWarning` or `ContextCompacted` events
