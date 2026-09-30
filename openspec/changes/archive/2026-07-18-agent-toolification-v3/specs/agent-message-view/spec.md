## ADDED Requirements

### Requirement: AgentMessage View Method

The `AgentMessage` type MUST expose a method `fn llm_visible(&self) -> bool` that returns whether the message SHOULD be visible to the LLM in conversation context. The method MUST be `O(1)` and side-effect free.

#### Scenario: System Message Visibility

- **WHEN** an `AgentMessage` is a system prompt
- **THEN** `llm_visible()` SHALL return `true`

#### Scenario: Internal Trace Message

- **WHEN** an `AgentMessage` is an internal trace (e.g., telemetry span, debug log)
- **THEN** `llm_visible()` SHALL return `false`

#### Scenario: Performance Contract

- **WHEN** `llm_visible()` is called in a tight loop over 10,000 messages
- **THEN** total elapsed time MUST be under 1ms on a developer workstation

### Requirement: MessageKind Discriminator

The `synthia-agent` crate MUST export a `MessageKind` enum with at least the variants: `System`, `User`, `Assistant`, `ToolCall`, `ToolResult`. `AgentMessage` MUST expose a `kind() -> MessageKind` accessor.

#### Scenario: Default Visibility Mapping

- **WHEN** `AgentMessage::kind()` returns `System` or `User` or `Assistant`
- **THEN** `llm_visible()` SHALL return `true` by default

#### Scenario: Tool Message Visibility

- **WHEN** `AgentMessage::kind()` returns `ToolCall` or `ToolResult`
- **THEN** `llm_visible()` SHALL return `true` only if the tool call result is non-empty