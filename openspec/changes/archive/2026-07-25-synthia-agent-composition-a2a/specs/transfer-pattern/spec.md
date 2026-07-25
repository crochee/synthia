# Spec: transfer-pattern

## ADDED Requirements

### Requirement: transfer_bidirectional function
`transfer_bidirectional(agent_a, agent_b_url, transport)` SHALL:
- Register `SendMessageTool` (for `agent_b_url`) in `agent_a.tool_registry`
- Register `SendMessageStreamTool` (for `agent_b_url`) in `agent_a.tool_registry`
- Agent B's side SHALL configure its own reference to A (symmetric)

#### Scenario: bidirectional tool registration
- **WHEN** `transfer_bidirectional(agent_a, agent_b_url, transport)` is called
- **THEN** `agent_a` has `SendMessageTool` and `SendMessageStreamTool` targeting `agent_b_url` in its tool registry

### Requirement: HandoffTool removal
`HandoffTool` SHALL be removed and replaced by `SendMessageTool` (A2A). `HandoffTool` was a fire-and-forget single-direction message; `SendMessage` SHALL support the full Task lifecycle.

#### Scenario: handoff tool replaced by send message
- **WHEN** the codebase is compiled after this change
- **THEN** `HandoffTool` no longer exists and all handoff semantics are handled by `SendMessageTool` with full Task lifecycle support
