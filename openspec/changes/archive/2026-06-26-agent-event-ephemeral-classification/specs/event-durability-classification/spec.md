## ADDED Requirements

### Requirement: AgentEvent durability method

The `AgentEvent` enum SHALL expose a method `is_durable(&self) -> bool`
that returns `true` for event variants whose replay mutates `LoopContext`
or `TurnTask` state, and `false` for variants that are observable
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

#### Scenario: Unknown event type defaults to durable
- **WHEN** `is_durable_event_type("UnknownType")` is called with a string
  that does not match any known event type constant
- **THEN** the function returns `true` (safe default: process unknown events)

---

### Requirement: Classification consistency invariant

The `AgentEvent::is_durable()` method and the `is_durable_event_type(&str)` lookup SHALL produce identical results for the same event. A unit test SHALL assert this invariant for every variant.

#### Scenario: Consistency test passes
- **WHEN** the test iterates all `AgentEvent` variants
- **THEN** for each variant, `is_durable()` matches
  `is_durable_event_type(serialized_type_tag)`
