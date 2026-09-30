<!--
Raw capture of brainstorming output.

本檔原樣捕捉 brainstorming 的產出，不強制結構。
Skill 的自然產出通常是 decision log 格式（背景 → 決議鏈 Q1-Qn → 設計取捨），
但依對話內容可能有不同組織方式。

design.md 從本檔萃取並重新整理為結構化設計文件。

不要將本檔的內容複製到 design.md — design.md 是獨立的重組產物，
兩者互補但不重疊。
-->

# Brainstorm: Subagent Event Streaming

## Background

Change A (`server-v2-session-controller`) established a per-session controller, persistent event log, and V2 REST API. The next step is to make subagents observable by multiple clients, which requires bridging child session events back into the parent session stream.

Current gaps:
- `SessionMetadata` / `types::Session` have no `parent_id`.
- `SubagentManager::session_id` is never written; `current_session_id()` returns `""`.
- `run_subagent` is a placeholder; no real child session is created.
- `AgentTool` cannot access `SessionManager` or `SessionController`.
- Subagent events are dropped or never constructed.
- No channel connects a child session's event stream to the parent's broadcaster.

## Decision Chain

**Q1: Should a subagent be an independent persistent session or a runtime-only AgentInstance?**
- Option A: Independent persistent session with `parent_id`.
- Option B: Runtime-only inside parent.
- Option C: Hybrid runtime with optional persistence.
- **Decision: A.** Subagents get their own session metadata, events.jsonl, and controller. This enables direct observation, replay, and user isolation.

**Q2: How should clients observe subagent progress?**
- Option A: Parent stream includes all child events.
- Option B: Child stream only; parent lists children.
- Option C: Both (parent shows lifecycle + child stream link).
- **Decision: A.** Mirror child events into the parent stream so a client watching the parent sees the entire session tree.

**Q3: What event granularity and envelope?**
- Option A: Mirror all child events with `child_session_id`.
- Option B: Mirror only lifecycle events.
- Option C: Lifecycle + high-level progress only.
- **Decision: A.** Forward every child event wrapped as `AgentEvent::SubagentEvent { child_session_id, event }`.

**Q4: Should child sessions expose their own `/events` endpoint?**
- Option A: Child has its own endpoint.
- Option B: Parent-only observation.
- Option C: Parent-only by default, child direct opt-in.
- **Decision: A.** A child session is a full session; `/api/v2/sessions/{child_id}/events` works without extra code.

**Q5: How should subagent sessions be created?**
- Option A: Tool-driven only (`AgentTool` / `run_subagent`).
- Option B: Add public `POST /sessions/{id}/subagents`.
- Option C: Both.
- **Decision: A.** Keep spawning tool-driven; this change focuses on event streaming, not spawn API.

**Q6: Which API shape for listing subagents?**
- Option A: Dedicated `GET /api/v2/sessions/{id}/subagents`.
- Option B: Filter on main list (`?parent_id={id}`).
- Option C: Both.
- **Decision: A.** Provide a dedicated subagent list endpoint.

## Design Trade-offs

**Approach 1: Full child SessionController with parent event channel (recommended)**
- `AgentTool` creates a real child session and child `SessionController` via a `SubagentSessionFactory` trait injected into `AgentRunConfig`.
- The child controller forwards wrapped child events to the parent controller's event channel.
- The parent controller persists and broadcasts mirrored events.
- **Pros:** child is independent and replayable; parent history is complete; decouples `AgentTool` from server types.
- **Cons:** requires an event channel on `SessionController` and factory injection.

**Approach 2: Agent stream forwarding**
- Child runs inside the parent agent stream and yields events directly into the parent stream.
- **Pros:** simpler, no controller bridge.
- **Cons:** child is not independently observable, contradicts Q1/Q4 decisions.

**Approach 3: EventStore async mirror**
- Background task watches child `events.jsonl` and forwards to parent.
- **Pros:** loose coupling.
- **Cons:** latency, ordering complexity, extra moving parts.

**Decision: Approach 1.**

## Key Design Points

- New `AgentEvent::SubagentEvent { child_session_id, event }` wrapper for parent stream.
- `SessionController` gains an internal forwarded-event channel (`event_tx` / `event_rx`).
- `SubagentSessionFactory::create_child` returns `ChildSessionHandle { session_id, user_id, parent_event_sender }`.
- Child events are persisted raw in `child/events.jsonl`; wrapped events are persisted in `parent/events.jsonl`.
- `GET /api/v2/sessions/{id}/subagents` returns child sessions with cursor pagination.
- `parent_id` added to `SessionMetadata`, `types::Session`, `SessionSummary`, and `SessionFilter` with `#[serde(default)]`.

## Risks

- Circular spawning: mitigated by existing `max_depth`.
- Parent gone before child: forwarding is best-effort, no panic.
- Event duplication: accepted for complete parent replay.
- Injection complexity: trait keeps `AgentTool` decoupled.
- Backward compatibility: `#[serde(default)]` on `parent_id` preserves old metadata.
