## Purpose

Forward child session events to the parent controller's event channel, enabling parent agents to observe subagent progress and outcomes through a unified event stream.
## Requirements
### Requirement: Child session events SHALL be wrapped and forwarded to the parent controller's event channel

When a child `SessionController` persists and broadcasts a child event, it SHALL additionally send an `AgentEvent::SubagentEvent { child_session_id, event }` to the parent controller's forwarded-event channel.

#### Scenario: Child emits a tool call event
- **WHEN** a child session emits `ToolCallStarted`
- **THEN** the parent controller receives `SubagentEvent { child_session_id, event: ToolCallStarted }`

---

### Requirement: The parent controller SHALL persist forwarded SubagentEvents into its own event log

The parent `SessionController` SHALL write every forwarded `SubagentEvent` to `{parent_session_path}/events.jsonl` using the same `EventStore::append` path as parent-generated events.

#### Scenario: Replay parent events after subagent activity
- **WHEN** a client requests `GET /api/v2/sessions/{parent_id}/events?last_seq=0`
- **THEN** the response includes `SubagentEvent` entries for all child events that occurred

---

### Requirement: The parent controller SHALL broadcast forwarded SubagentEvents to parent subscribers

After persisting a forwarded `SubagentEvent`, the parent controller SHALL send it to its own `EventBroadcaster` so all parent SSE/WS clients receive it.

#### Scenario: Multi-client parent observation
- **WHEN** two clients are subscribed to the parent `/events` stream
- **THEN** both clients receive the same `SubagentEvent` when a child event occurs

---

### Requirement: Event forwarding SHALL be best-effort and SHALL NOT panic if the parent channel is closed

If the parent controller has shut down and its forwarded-event channel is closed, the child controller SHALL log a warning and continue normal operation.

#### Scenario: Parent shuts down while child runs
- **WHEN** the parent controller closes its event channel while the child is still running
- **THEN** the child controller continues executing without panicking

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

