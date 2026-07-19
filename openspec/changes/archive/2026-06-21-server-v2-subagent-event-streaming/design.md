## Context

Change A (`server-v2-session-controller`) turned `synthia-server` into a multi-client-capable backend: one controller per session, append-only event persistence, V2 REST endpoints, and SSE/WS event streaming. The next logical step is to extend that model to subagents.

Currently, subagents are runtime-only `AgentInstance`s created by `AgentTool`. They have no persistent session, no controller, and no event stream. `SubagentManager::session_id` is never written; `run_subagent` is a placeholder; subagent events are dropped. Clients cannot observe what a subagent is doing unless they happen to be watching raw LLM output, which is neither structured nor multi-client friendly.

This design makes every subagent a first-class persistent session with a `parent_id`, bridges its event stream back into the parent session, and exposes child sessions through the existing V2 API surface.

## Goals / Non-Goals

**Goals:**
- Every subagent is created as an independent persistent session with `parent_id`.
- Child session events are mirrored into the parent session event stream.
- A child session is observable directly via the standard V2 `/events` endpoint.
- Add `GET /api/v2/sessions/{id}/subagents` to list child sessions.
- Keep subagent spawning tool-driven (`AgentTool`); no new public spawn endpoint.
- Preserve backward compatibility for existing session metadata.

**Non-Goals:**
- Redesigning the LLM subagent execution loop beyond what is needed to create a real session/controller.
- UI/TUI rendering of subagent events.
- Billing, quota, or authorization changes beyond existing user isolation.
- Public HTTP endpoint to spawn subagents manually.

## Decisions

### D1: Subagent session model
- **Choice:** Subagents run as independent persistent sessions with `parent_id`.
- **Reason:** Enables direct observation, replay, user isolation, and reuse of the existing controller/event-store infrastructure.
- **Alternatives considered:**
  - Runtime-only inside parent: simpler but not independently observable.
  - Hybrid optional persistence: adds complexity without a concrete use case.

### D2: Parent stream mirroring
- **Choice:** Every child event is wrapped as `AgentEvent::SubagentEvent { child_session_id, event }` and forwarded to the parent controller's event channel.
- **Reason:** A client watching the parent sees the entire session tree without opening multiple connections.
- **Alternatives considered:**
  - Child stream only: forces clients to manage many connections.
  - Lifecycle-only mirroring: hides useful subagent progress detail.

### D3: Parent event persistence
- **Choice:** The parent controller persists forwarded `SubagentEvent`s into its own `events.jsonl`.
- **Reason:** Replay (`GET /events?last_seq=N`) must include subagent history.
- **Trade-off:** Child events are duplicated in parent storage; accepted for complete replay.

### D4: Decoupling AgentTool from server types
- **Choice:** Introduce a `SubagentSessionFactory` trait injected through `AgentRunConfig`; server provides the implementation.
- **Reason:** Keeps `synthia-agent` crate decoupled from Axum/server internals while allowing real child session creation.
- **Alternatives considered:**
  - Direct `AppState` reference in `AgentTool`: creates crate dependency cycle and coupling.
  - Callback closure: harder to test and configure.

### D5: Event forwarding channel
- **Choice:** `SessionController` exposes an internal `event_tx`/`event_rx` pair for forwarded child events; child controllers receive `parent_event_sender`.
- **Reason:** Ensures parent persistence and broadcasting happen through the same path as parent-generated events.
- **Alternatives considered:**
  - Direct broadcaster send: would not persist to parent `events.jsonl`.
  - Direct cross-session `EventStore::append`: breaks controller encapsulation.

### D6: Subagent listing API
- **Choice:** Dedicated `GET /api/v2/sessions/{id}/subagents`.
- **Reason:** Clear, discoverable, and naturally supports cursor pagination.
- **Alternatives considered:**
  - `?parent_id=` filter on main list: less discoverable for this use case.
  - Both: overkill for the initial change.

## Risks / Trade-offs

- **[Risk]** Circular subagent spawning → Mitigation: existing `max_depth` enforcement in `AgentTool` is preserved.
- **[Risk]** Parent controller shuts down while child still runs → Mitigation: forwarding is best-effort; a closed channel logs and continues without panic.
- **[Trade-off]** Event duplication (child events in both child and parent `events.jsonl`) → Accepted so parent replay is self-contained.
- **[Risk]** `AgentRunConfig` serialization with a trait-object factory field → Mitigation: mark the field `#[serde(skip_serializing, skip_deserializing)]` with a custom default returning `None`.
- **[Risk]** Large parent event streams if subagents are verbose → Mitigation: existing event replay and pagination remain; future work can add event-type filtering.

## Migration Plan

1. Deploy code with new `parent_id` field (backward-compatible due to `#[serde(default)]`).
2. Existing sessions continue to work without `parent_id`.
3. New subagent sessions created after deployment will have `parent_id` and emit `SubagentEvent` wrappers.
4. Rollback: revert code changes; old sessions remain readable because `parent_id` defaults to `None`.

## Open Questions

- Should `AgentEvent::SubagentEvent` support an optional `agent_path` field in addition to `child_session_id` for richer filtering?
- Should the parent stream also emit explicit `SubagentSpawnBegin`/`SubagentComplete` lifecycle wrappers, or is the first/last child event enough?
- Should child sessions inherit the parent's title or receive an auto-generated title (e.g., "Subagent of {parent}")?
