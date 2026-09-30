## Why

The replay harness (agent-turn-events-phase1, merged 2026-06-25) has no
explicit durable/ephemeral classification. `replay.rs` implicitly skips
unknown event types via `_ => {}`, making the classification implicit,
fragile (new variants require manual replay.rs update), and opaque to
external observers. This is P1-1 from the project roadmap: "AgentEvent 加
ephemeral 字段 (~100 行): replay 性能关键", borrowed from opencode's
`DurableDefinitions` vs `EphemeralDefinitions` pattern.

## What Changes

**AgentEvent durability classification**
- From: No explicit classification; replay.rs implicitly skips via `_ => {}`
- To: `AgentEvent::is_durable()` method + `ephemeral: bool` field on `PersistedEvent`
- Reason: Make classification explicit, single source of truth, backward compatible
- Impact: non-breaking (serde default handles old files)

**PersistedEvent gains `ephemeral` field**
- From: 6 fields (seq, aggregate, event_type, ts, source, payload)
- To: 7 fields (+ `ephemeral: bool` with `#[serde(default)]` = false)
- Reason: Persist the classification so replay can skip without parsing payload
- Impact: non-breaking (old JSONL without the field deserializes as durable)

**Replay skips ephemeral events explicitly**
- From: `apply_event` match with `_ => {}` for unknown types
- To: `if event.ephemeral { return; }` before match, plus match arms for durable types
- Reason: Explicit skip; future ephemeral events auto-skipped without replay.rs changes
- Impact: non-breaking (behavior unchanged; durable events still processed)

## Capabilities

### New Capabilities

- `event-durability-classification`: Classifies agent events as durable
  (state-changing, must be replayed) or ephemeral (observable, skippable)
  at both the `AgentEvent` type layer and the `PersistedEvent` persistence
  layer.

### Modified Capabilities

- `jsonl-event-sourcing`: `PersistedEvent` gains an `ephemeral: bool` field
  (with `#[serde(default)]` = false) so the durability classification is
  persisted alongside the event payload.
- `session-replay-harness`: Replay harness now explicitly skips ephemeral
  events via the persisted `ephemeral` field instead of implicit
  `_ => {}` match fallback.

## Impact

- **Affected code**:
  - `crates/synthia-agent/src/events/event_enum.rs` — add `is_durable()` method
  - `crates/synthia-agent/src/events/persisted.rs` — add `is_durable_event_type()`
  - `crates/synthia-session/src/store/events.rs` — add `ephemeral` field to
    `PersistedEvent`, update `EventStore::append` signature
  - `crates/synthia-agent/src/replay.rs` — skip ephemeral events
  - Callers of `EventStore::append` (only `append_agent_event` and tests)
- **APIs**: `EventStore::append` signature gains `ephemeral: bool` parameter
- **Dependencies**: none new
- **Systems**: replay harness, telemetry, tests
