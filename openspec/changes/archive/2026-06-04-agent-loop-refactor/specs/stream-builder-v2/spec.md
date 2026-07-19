## ADDED Requirements

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

StepExecutor SHALL provide step_sample(), step_tool_execute(), step_compact_check() methods that execute each phase of the agent loop in order.

#### Scenario: step_sample executes LLM request
- **WHEN** step_sample() is called with a LoopContext
- **THEN** It SHALL send messages to the LLM provider
- **AND** yield LlmRequestStarted and LlmStreamDelta events
- **AND** return a SamplingResult containing text and tool calls

#### Scenario: step_tool_execute runs tools
- **WHEN** step_tool_execute() is called with tool_calls
- **THEN** It SHALL execute each tool via tool_registry
- **AND** yield ToolCallStarted and ToolCallCompleted events
- **AND** return ToolResults for each tool call

#### Scenario: step_compact_check evaluates token budget
- **WHEN** step_compact_check() is called after any message addition
- **THEN** It SHALL check if token count exceeds budget thresholds
- **AND** trigger compaction if MustCompact status is detected
- **AND** yield TokenBudgetWarning or ContextCompacted events

---

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