## Why

Subagents today are invisible runtime entities: `AgentTool` creates an `AgentInstance`, `run_subagent` is a placeholder, and no events reach the parent session stream. After Change A gave every session a persistent event log and a multi-client controller, the next step is to make subagents first-class sessions so clients can observe their progress in real time and replay it later.

## What Changes

**Subagent session model**
- From: subagents are runtime-only `AgentInstance`s with no persistent session or event stream.
- To: every subagent is a persistent session with `parent_id`, stored under the same user namespace, with its own `SessionController` and `events.jsonl`.
- Reason: reuse the controller/event-store infrastructure and enable direct observation.
- Impact: non-breaking for existing sessions; new subagent metadata gains `parent_id`.

**Parent event stream**
- From: parent session stream contains only parent agent events.
- To: parent stream also receives every child event wrapped as `AgentEvent::SubagentEvent { child_session_id, event }`.
- Reason: a single SSE/WS connection on the parent shows the whole session tree.
- Impact: new SSE event name `subagent_event`; existing clients ignoring unknown names remain safe.

**SubagentSessionFactory injection**
- From: `AgentTool` has no access to `SessionManager`/`SessionController`.
- To: `AgentRunConfig` carries an optional `Arc<dyn SubagentSessionFactory>`; server provides an implementation backed by `AppState`.
- Reason: decouple agent logic from server wiring while enabling real child session creation.
- Impact: production `AgentRunConfig` literals updated to pass `None`; server sets the factory.

**Subagent listing endpoint**
- From: no way to list child sessions of a session.
- To: `GET /api/v2/sessions/{id}/subagents` returns child sessions with cursor pagination.
- Reason: clients need to discover subagents and optionally open a direct child stream.
- Impact: new V2 route; response reuses `SessionSummary` with `parent_id`.

## Capabilities

### New Capabilities
- `subagent-session-model`: Subagents are created as persistent sessions with `parent_id` and user isolation.
- `subagent-event-bridge`: Child session events are mirrored into the parent session event stream with `child_session_id` wrapping.
- `subagent-listing`: `GET /api/v2/sessions/{id}/subagents` lists child sessions.

### Modified Capabilities
- `v2-session-api`: Response schemas (`SessionSummary`) and filters gain `parent_id`; no endpoint behavior is removed.

## Impact

- `crates/synthia-session`: `SessionMetadata`, `types::Session`, `SessionSummary`, `SessionFilter` gain `parent_id`; `SessionManager` gains `create_child` and `list_children`.
- `crates/synthia-agent`: `AgentRunConfig` gains `subagent_session_factory`; `AgentTool`/`run_subagent` create real child sessions; new `AgentEvent::SubagentEvent` variant.
- `crates/synthia-server`: `SubagentSessionFactory` implementation backed by `AppState`; `SessionController` gains forwarded-event channel; new V2 route for subagent listing.
