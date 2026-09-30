## ADDED Requirements

### Requirement: Foreground subagent completion events SHALL be distinguishable from background completion notifications

When a subagent completes in the foreground, the result SHALL be returned as the direct `ToolOutput` of the `task` tool call. When a subagent completes in the background, the parent controller SHALL receive the result through the existing `SubagentEvent` forwarding path so it can be injected into the parent context.

#### Scenario: Foreground task completes
- **WHEN** a foreground subagent finishes
- **THEN** the result SHALL be returned synchronously in the `task` tool output
- **AND THEN** no synthetic `SubagentEvent` completion message is injected into the parent context

#### Scenario: Background task completes
- **WHEN** a background subagent finishes
- **THEN** the parent SHALL receive the final child events through `SubagentEvent` forwarding
- **AND THEN** the main loop SHALL inject a synthetic `<task>` result message into `ctx.messages`
