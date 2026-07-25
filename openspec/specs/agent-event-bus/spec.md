# agent-event-bus Specification

## Purpose
TBD - created by archiving change simplify-agent-event-stream. Update Purpose after archive.
## Requirements
### Requirement: AgentEvent top-level enum structure

The `AgentEvent` enum MUST expose exactly five top-level variants:
`Model(ContentPart)`, `ModelDone(SamplingResult)`,
`System(SystemEvent)`, `Agent(AgentMeta, Box<AgentEvent>)`,
and `Hook(HookEvent)`.

#### Scenario: All 5 top-level variants are constructible
- **WHEN** code constructs an `AgentEvent`
- **THEN** the construction MUST succeed via one of the five documented variants
- **AND** no legacy variant name (e.g. `LlmStreamDelta`, `LlmReasoningDelta`, `LlmResponseComplete`, `Thinking`, `ToolCallStarted`, `ToolCallCompleted`, `ToolCallSkipped`, `ToolCallError`, `IterationStarted`, `IterationCompleted`, `Checkpoint`, `StateChange`, `ContextCompacted`, `RecoveryApplied`, `Finish`, `GuardianWarning`, `LoopWarning`, `TokenBudgetWarning`, `TokenBudgetNotice`, `SteeringReceived`, `HookError`, `GuardianConfirmationRequest`, `EditConflict`, `SelfReflection`, `SubagentSpawnBegin`, `SubagentSpawnEnd`, `SubagentMessage`, `SubagentComplete`, `SubagentCompleted`, `SubagentEvent`, `Status`, `Warning`, `Progress`, `Custom`) is constructible

---

### Requirement: Model variant passes Provider ContentPart through

The `Model(ContentPart)` variant MUST accept any value of the Provider `ContentPart` enum without re-shaping or re-classification.

#### Scenario: Text and Reasoning are passed through as distinct ContentPart variants
- **WHEN** a Provider streams a text chunk
- **THEN** the agent emits `AgentEvent::Model(ContentPart::Text(TextContent))`
- **AND** when a Provider streams a reasoning chunk
- **THEN** the agent emits `AgentEvent::Model(ContentPart::Reasoning(ReasoningContent))`

#### Scenario: Tool calls and results pass through as ContentPart variants
- **WHEN** a Provider yields a tool call
- **THEN** the agent emits `AgentEvent::Model(ContentPart::ToolUse(ToolUse))`
- **AND** when a tool execution completes
- **THEN** the agent emits `AgentEvent::Model(ContentPart::ToolResult(ToolResult))`

#### Scenario: Attachments pass through as ContentPart variants
- **WHEN** a Provider yields an image, audio, or resource attachment
- **THEN** the agent emits `AgentEvent::Model(ContentPart::Image | Audio | Resource(...))`

---

### Requirement: SystemEvent enumerates lifecycle, diagnostics, and terminal events

The `SystemEvent` enum MUST include these variants:
`SessionStarted`, `SessionEnded`, `SessionInterrupted`, `Progress`,
`Warning`, `Recovery`, `Usage`.

#### Scenario: Session lifecycle events are emitted as SystemEvent
- **WHEN** a session starts
- **THEN** the agent emits `AgentEvent::System(SystemEvent::SessionStarted)`
- **AND** when a session ends
- **THEN** the agent emits `AgentEvent::System(SystemEvent::SessionEnded)`
- **AND** when a session is interrupted
- **THEN** the agent emits `AgentEvent::System(SystemEvent::SessionInterrupted)`

#### Scenario: Diagnostics events are emitted as SystemEvent
- **WHEN** progress updates, warnings, recoveries, or usage stats are produced
- **THEN** the agent emits the corresponding `SystemEvent::Progress`, `Warning`, `Recovery`, or `Usage` variant

---

### Requirement: AgentMeta structure for subagent traces

The `AgentMeta` struct MUST contain exactly three fields:
`parent_session_id: String`, `child_session_id: String`, and
`parent_depth: usize`.

#### Scenario: Subagent events carry AgentMeta with both session ids
- **WHEN** a child agent emits an event to the parent
- **THEN** the parent receives `AgentEvent::Agent(AgentMeta, Box<AgentEvent>)`
- **AND** `AgentMeta.parent_session_id` identifies the spawning parent session
- **AND** `AgentMeta.child_session_id` identifies the child session that produced the event
- **AND** `AgentMeta.parent_depth` indicates the nesting depth

---

### Requirement: HookEvent covers external injection including Custom events

The `HookEvent` enum MUST include these variants:
`Message`, `ConfirmRequest`, `ConfirmResponse`, `Custom`.

#### Scenario: Steering messages and confirmations are emitted as HookEvent
- **WHEN** a user steering message arrives
- **THEN** the agent emits `AgentEvent::Hook(HookEvent::Message)`
- **AND** when a guardian confirmation is requested or responded to
- **THEN** the agent emits `AgentEvent::Hook(HookEvent::ConfirmRequest | ConfirmResponse)`
- **AND** when an extension plugin produces a custom event
- **THEN** the agent emits `AgentEvent::Hook(HookEvent::Custom)`

---

### Requirement: Wire format uses A2A Part::data for typed JSON payloads

Every `AgentEvent` payload MUST be translated to the wire as
`a2a_types::Part::data(serde_json::Value)` with a discriminator field
`"kind"` set to one of the documented values.

#### Scenario: All non-text payloads use Part::data with a kind discriminator
- **WHEN** the agent-to-A2A mapping translates a Model, System, Agent, or Hook event
- **THEN** the resulting A2A Message MUST contain `Part::data({ kind: "<variant>", ...payload })`
- **AND** no `metadata.segment_type` string discriminator is emitted
- **AND** no empty `Part::text("")` marker messages are emitted as end-of-stream signals

#### Scenario: Wire kind values follow documented enum
- **WHEN** the mapping emits a `Part::data` payload
- **THEN** the `kind` field MUST be one of:
  `text_delta`, `thinking_delta`, `tool_call`, `tool_result`,
  `response_complete`, `progress`, `warning`, `recovery`, `usage`,
  `agent_meta`, `hook_message`, `hook_confirm_request`,
  `hook_confirm_response`, or a custom hook kind string

#### Scenario: StatusUpdate state is derived from SessionEvent
- **WHEN** the mapping translates a `SystemEvent::SessionStarted`
- **THEN** the A2A StatusUpdate state is `Working`
- **AND** when translating `SessionEnded(SessionEndReason::Completed)`
- **THEN** the state is `Completed`
- **AND** when translating `SessionEnded(SessionEndReason::Error(_))`
- **THEN** the state is `Failed`
- **AND** when translating `SessionEnded(SessionEndReason::Cancelled)`
- **THEN** the state is `Canceled`
- **AND** when translating `SessionInterrupted`
- **THEN** the state is `InputRequired`

---

### Requirement: Part::text usage is restricted to A2A StatusUpdate messages

`Part::text` MUST only be used for the human-readable `message` field
of an A2A `StatusUpdate`. All other event payloads MUST use
`Part::data` or `Part::file`.

#### Scenario: AgentEvent payload never uses Part::text
- **WHEN** the mapping translates any `AgentEvent` variant to wire
- **THEN** no `Part::text` is emitted as a payload part
- **EXCEPT** for the optional `message` field on A2A `StatusUpdate` carrying human-readable status text

---

### Requirement: TaskState is derived exclusively from SessionEvent

The mapping from `AgentEvent` to A2A `TaskState` MUST be computed
solely from `SystemEvent::SessionStarted | SessionEnded | SessionInterrupted`.

#### Scenario: No AgentStatus or Status variant exists in AgentEvent
- **WHEN** the `AgentEvent` enum is enumerated
- **THEN** it does NOT contain a `Status(AgentStatus)` variant
- **AND** task status is derived entirely from `SystemEvent::Session*` variants
