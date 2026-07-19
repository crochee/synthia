## ADDED Requirements

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
