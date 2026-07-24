# event-durability-classification Specification

## Purpose
TBD - created by archiving change agent-event-ephemeral-classification. Update Purpose after archive.
## Requirements
### Requirement: AgentEvent durability method

The `AgentEvent` enum SHALL expose a method `is_durable(&self) -> bool`
that returns `true` for event paths whose replay mutates `LoopContext`
or `TurnTask` state, and `false` for paths that are observable
side-effects only.

#### Scenario: Durable event classification
- **WHEN** `AgentEvent::is_durable()` is called on `SessionStarted`,
  `SessionEnded`, `TurnStarted`, `TurnCompleted`, `TurnFailed`,
  `SampleCompleted`, `ToolCallIssued`, `ToolResultReceived`,
  `LlmRequestStarted`, `LlmResponseComplete`, `ToolCallStarted`,
  `ToolCallCompleted`, `ToolCallError`, `ToolCallSkipped`,
  `IterationStarted`, `ContextCompacted`, `Checkpoint`, `StateChange`,
  `RecoveryApplied`, `Status`, `SteeringReceived`,
  `GuardianConfirmationRequest`, `SubagentSpawnBegin`,
  `SubagentSpawnEnd`, `SubagentComplete`, or `Finish`
- **THEN** the method returns `true`

#### Scenario: Ephemeral event classification
- **WHEN** `AgentEvent::is_durable()` is called on `LlmStreamDelta`,
  `LlmReasoningDelta`, `Thinking`, `Progress`, `Warning`, `LoopWarning`,
  `GuardianWarning`, `TokenBudgetNotice`, `TokenBudgetWarning`,
  `IterationCompleted`, `SessionInterrupted`, `HookError`,
  `SelfReflection`, `SubagentMessage`, or `SubagentEvent`
- **THEN** the method returns `false`

#### Scenario: Durable paths in the restructured AgentEvent
- **WHEN** `AgentEvent::is_durable()` is called on:
  - `Model(ContentPart::Text(_))`
  - `Model(ContentPart::ToolUse(_))`
  - `Model(ContentPart::ToolResult(_))`
  - `Model(ContentPart::Resource(_))`
- **THEN** the method returns `true`

#### Scenario: Ephemeral paths in the restructured AgentEvent
- **WHEN** `AgentEvent::is_durable()` is called on:
  - `Model(ContentPart::Reasoning(_))`
  - `Model(ContentPart::Image(_))`
  - `Model(ContentPart::Audio(_))`
  - `ModelDone(_)`
  - `System(_)` (any `SystemEvent` variant)
  - `Agent(_, _)` (any nested subagent event)
  - `Hook(_)` (any `HookEvent` variant)
- **THEN** the method returns `false`

#### Scenario: Reasoning is not durable but is wired
- **WHEN** `is_durable()` is called on `Model(ContentPart::Reasoning(_))`
- **THEN** it returns `false`
- **AND** the variant is still emitted on the wire so that clients can display reasoning content

---

### Requirement: Durable event type lookup

The persistence layer SHALL expose a function
`is_durable_event_type(event_type: &str) -> bool` that returns the same
result as `AgentEvent::is_durable()` for the corresponding event type
string. This function is the persistence-layer projection of the
in-memory classification.

#### Scenario: Lookup matches method for all variants
- **WHEN** every `AgentEvent` variant is serialized to extract its `type`
  tag and `is_durable_event_type(type_tag)` is called
- **THEN** the result matches `AgentEvent::is_durable()` for that variant

---

### Requirement: Classification consistency invariant

The `AgentEvent::is_durable()` method and the `is_durable_event_type(&str)` lookup SHALL produce identical results for the same event. A unit test SHALL assert this invariant for every variant.

#### Scenario: Consistency test passes
- **WHEN** the test iterates all `AgentEvent` variants
- **THEN** for each variant, `is_durable()` matches
  `is_durable_event_type(serialized_type_tag)`
