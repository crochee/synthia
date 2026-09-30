## MODIFIED Requirements

### Requirement: Child session events SHALL be wrapped and forwarded to the parent controller's event channel

When a child `SessionController` persists and broadcasts a child event, it MUST additionally send an `AgentEvent::Agent(AgentMeta, Box<AgentEvent>)` to the parent controller's forwarded-event channel, where `AgentMeta.parent_session_id` is the spawning parent session and `AgentMeta.child_session_id` is the producing child session.

#### Scenario: Child emits a model event forwarded as Agent
- **WHEN** a child session emits `AgentEvent::Model(ContentPart::Text(_))`
- **THEN** the parent controller receives `AgentEvent::Agent(AgentMeta { parent_session_id, child_session_id, parent_depth }, Box::new(Model(ContentPart::Text(_))))`

#### Scenario: Child emits a session-end event forwarded as Agent
- **WHEN** a child session emits `AgentEvent::System(SystemEvent::SessionEnded(_))`
- **THEN** the parent controller receives `AgentEvent::Agent(AgentMeta { ... }, Box::new(System(SystemEvent::SessionEnded(_))))`

---

### Requirement: The parent controller SHALL persist forwarded Agent events into its own event log

The parent `SessionController` MUST write every forwarded `AgentEvent::Agent` to `{parent_session_path}/events.jsonl` using the same `EventStore::append` path as parent-generated events.

#### Scenario: Replay parent events after subagent activity
- **WHEN** a client requests `GET /api/v2/sessions/{parent_id}/events?last_seq=0`
- **THEN** the response includes `AgentEvent::Agent` entries for all child events that occurred

---

### Requirement: The parent controller SHALL broadcast forwarded Agent events to parent subscribers

After persisting a forwarded `AgentEvent::Agent`, the parent controller MUST send it to its own `EventBroadcaster` so all parent SSE/WS clients receive it.

#### Scenario: Multi-client parent observation
- **WHEN** two clients are subscribed to the parent `/events` stream
- **THEN** both clients receive forwarded `AgentEvent::Agent` entries

---

## REMOVED Requirements

### Requirement: SubagentEvent wrapper variant

**Reason**: The legacy `AgentEvent::SubagentEvent { child_session_id, event }` variant is replaced by `AgentEvent::Agent(AgentMeta, Box<AgentEvent>)`, which carries both `parent_session_id` and `child_session_id` plus `parent_depth` in a structured `AgentMeta`. The new shape generalises started/completed/failed semantics through nested `SystemEvent::SessionStarted | SessionEnded` events rather than dedicated wrapper variants.

**Migration**: Producers MUST emit `AgentEvent::Agent(AgentMeta { parent_session_id, child_session_id, parent_depth }, Box::new(inner))`. Consumers MUST match `AgentEvent::Agent(meta, inner)` instead of `AgentEvent::SubagentEvent { child_session_id, event }`. To detect child lifecycle boundaries, consumers examine `inner` for `SystemEvent::SessionStarted` (start), `SessionEnded(Completed)` (success), or `SessionEnded(Error(_) | ...)` (failure).