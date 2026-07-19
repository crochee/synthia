# stream-builder-v2 Specification

## Purpose
TBD - created by archiving change agent-loop-refactor. Update Purpose after archive.
## Requirements
### Requirement: StreamBuilder shall provide a fluent builder API for agent configuration

The StreamBuilder SHALL provide a builder pattern that allows configuring all agent components (provider, tool_registry, hook_registry, context_assembler, model_router) through method chaining.

#### Scenario: Basic StreamBuilder construction
- **WHEN** User creates a StreamBuilder with AgentConfig
- **THEN** StreamBuilder SHALL provide `with_provider()`, `with_tool_registry()`, `with_hook_registry()`, `with_context_assembler()`, `with_model_router()` methods
- **AND** The builder SHALL return `Self` from each method to enable chaining

#### Scenario: StreamBuilder build method
- **WHEN** User calls `build()` after configuring all required components
- **THEN** StreamBuilder SHALL return an `AgentLoop` instance ready to execute
- **AND** Missing required components SHALL result in a clear error message

---

### Requirement: LoopContext shall maintain all state for a single agent iteration

LoopContext SHALL be the single source of truth for iteration state including session_id, iteration number, messages, end_reason, cumulative_tokens, recent_tool_results, and needs_compact flag.

#### Scenario: LoopContext initialization
- **WHEN** A new agent session starts
- **THEN** LoopContext SHALL be created with session_id and empty messages
- **AND** iteration SHALL be initialized to 0
- **AND** end_reason SHALL be None

#### Scenario: LoopContext increment_iteration
- **WHEN** Agent completes an iteration
- **THEN** LoopContext SHALL increment iteration counter
- **AND** maintain all message history

---

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

### Requirement: Main loop shall iterate until termination condition

The main loop SHALL continue iterating while not cancelled and iteration < max_iterations, checking end_reason at each iteration.

#### Scenario: Normal completion
- **WHEN** LLM response contains no tool calls
- **THEN** Loop SHALL set end_reason to Completed
- **AND** exit the loop

#### Scenario: Max iterations reached
- **WHEN** iteration reaches max_iterations
- **THEN** Loop SHALL set end_reason to MaxIterationsReached
- **AND** exit the loop

#### Scenario: Cancellation
- **WHEN** CancellationToken is cancelled
- **THEN** Loop SHALL save current state
- **AND** set end_reason to Cancelled
- **AND** exit the loop

---

### Requirement: Legacy build_stream shall be preserved for backup

The existing legacy.rs::build_stream() SHALL be preserved unchanged as a backup implementation during the transition period.

#### Scenario: Legacy backup verification
- **WHEN** New StreamBuilder implementation is tested
- **THEN** Legacy implementation SHALL be available for comparison
- **AND** Legacy SHALL produce identical results for same inputs
- **AND** Legacy SHALL be removed only after new implementation passes all tests

### Requirement: StreamBuilder shall use synthia_guardian::LoopDetectorSet for loop detection

The `StreamBuilder` SHALL use `synthia_guardian::LoopDetectorSet` as its loop detection backend. It MUST NOT import or use any `LoopDetectorSet` defined in `synthia-agent`. The local `synthia-agent/src/stream_builder/loop_detection.rs` module SHALL be removed.

#### Scenario: Loop detection dependency direction
- **WHEN** `StreamBuilder` is constructed
- **THEN** its loop detection dependency SHALL be `synthia_guardian::LoopDetectorSet`
- **AND** it SHALL NOT use `synthia_agent::stream_builder::loop_detection::LoopDetectorSet`

#### Scenario: Removal of local loop detection module
- **WHEN** `cargo build -p synthia-agent` runs after migration
- **THEN** the file `crates/synthia-agent/src/stream_builder/loop_detection.rs` SHALL NOT exist
- **AND** `synthia-agent/src/stream_builder/mod.rs` SHALL NOT export any `loop_detection` submodule

---

### Requirement: StreamBuilder shall handle LoopAction::RequirePermission

The `StepExecutor` SHALL inspect `(LoopStatus, Option<LoopAction>)` from `LoopDetectorSet::check()` and invoke `synthia_permission::Permission::ask()` when receiving `LoopAction::RequirePermission`. The user's decision (allow / deny) SHALL determine whether the tool is executed.

#### Scenario: Doom loop triggers permission ask
- **WHEN** `check()` returns `(LoopStatus::Detected, Some(LoopAction::RequirePermission))`
- **THEN** `StepExecutor` SHALL call `permission.ask(DoomLoop { tool, args })`
- **AND** if the user allows, the tool SHALL be executed
- **AND** if the user denies, the tool SHALL be skipped and the loop SHALL continue

#### Scenario: Standard block skips execution
- **WHEN** `check()` returns `(LoopStatus::Detected, Some(LoopAction::Block))` or `(LoopStatus::Detected, Some(LoopAction::HardBlock))`
- **THEN** `StepExecutor` SHALL NOT execute the tool
- **AND** it SHALL log a warning with the detector name
- **AND** it SHALL continue to the next iteration

#### Scenario: Warning is logged but does not block
- **WHEN** `check()` returns `(LoopStatus::Warning, Some(LoopAction::Warn))`
- **THEN** `StepExecutor` SHALL log a warning
- **AND** it SHALL proceed with tool execution

---

### Requirement: StreamBuilder shall pass iteration count to LoopDetectorSet

The `StepExecutor` SHALL pass the current iteration count from `LoopContext` to `LoopDetectorSet::check()` on every call. This enables the `GlobalCircuitDetector` to use the iteration argument instead of maintaining its own counter.

#### Scenario: Iteration argument propagation
- **WHEN** `StepExecutor` calls `check()` for a tool call
- **THEN** the third argument SHALL be `ctx.iteration`
- **AND** the value SHALL match the iteration count in `LoopContext`

