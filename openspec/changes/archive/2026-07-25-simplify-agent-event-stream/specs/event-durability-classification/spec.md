## MODIFIED Requirements

### Requirement: AgentEvent durability method

The `AgentEvent` enum MUST expose a method `is_durable(&self) -> bool`
that returns `true` for event paths whose replay mutates `LoopContext`
or `TurnTask` state, and `false` for paths that are observable
side-effects only.

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

## REMOVED Requirements

### Requirement: Unknown event safe default as durable

**Reason**: After the AgentEvent restructuring there are no "unknown" variants — the enum is exhaustively defined with five top-level variants and four documented sub-enums. The safe-default rule was a workaround for the legacy 32-variant enum and is no longer applicable.

**Migration**: Consumers MUST pattern-match all five top-level variants exhaustively. The `is_durable()` method now returns an explicit `bool` for every path; there is no fallthrough default.