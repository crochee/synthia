<!--
Raw capture of brainstorming output for the agent-event-ephemeral-classification change.
Decision log format: background → decision chain → design trade-offs.
-->

# Brainstorm: AgentEvent Ephemeral Classification (P1-1)

## Background

The agent-turn-events-phase1 change (merged 2026-06-25) introduced a JSONL-based
event persistence layer and a replay harness (`replay.rs`). The harness reads
`events.jsonl` line-by-line and projects loop state + turn tasks.

**Problem**: The replay harness has no explicit notion of which events are
*durable* (state-changing, must be replayed) vs *ephemeral* (observable but
non-state-changing, can be skipped). The current `apply_event` function in
`replay.rs` implicitly skips unknown event types via `_ => {}`, but this
classification is:

1. **Implicit**: Lives in replay.rs's match arms, not in the event type itself
2. **Fragile**: Every new `AgentEvent` variant requires a manual replay.rs update
3. **Opaque**: External observers (telemetry, UI, tests) cannot determine which
   events are safe to skip during replay
4. **Unscalable**: As the event catalog grows (~30 variants today), the
   implicit classification becomes a maintenance burden and a source of bugs
   (a new durable event forgotten in replay.rs silently breaks replay).

This maps to project memory P1-1: "AgentEvent 加 ephemeral 字段 (~100 行):
replay 性能关键" — borrowed from opencode's `DurableDefinitions` vs
`EphemeralDefinitions` pattern in `packages/core/src/session/event.ts`.

## Decision Chain

### Q1: Where should the durable/ephemeral classification live?

**Option A: Method on `AgentEvent` (`fn is_durable(&self) -> bool`)**
- Pro: Classification lives with the type, single source of truth
- Pro: No persistence format change
- Con: Doesn't help replay skip events at read time (still deserializes every line)
- Con: Doesn't help external observers who only see `PersistedEvent`

**Option B: `ephemeral: bool` field on `PersistedEvent`**
- Pro: Explicit at persistence layer, enables read-time filtering
- Pro: Backward compatible with `#[serde(default)]` (old events = durable)
- Con: Requires updating `EventStore::append` signature
- Con: Requires updating all `append_agent_event` callers
- Con: Redundant if the event_type string already determines durability

**Option C: Both (method on `AgentEvent` + field on `PersistedEvent`)**
- Pro: Classification is explicit at both layers
- Pro: The method is the source of truth; the field is its persisted projection
- Pro: Replay can skip ephemeral events by checking the field (no payload parse)
- Pro: External observers see the field without needing the `AgentEvent` enum
- Con: Slightly more code (~100 lines, matches estimate)
- Con: Two places to keep in sync (but the field is derived from the method, so
  it's a one-way flow: method → field)

**Decision: Option C** — two-layer classification. The method is the source of
truth; the persisted field is derived from it at append time. This is the
opencode pattern: `DurableDefinitions` (type union) + persisted `ephemeral`
flag.

### Q2: What is the durability classification of each `AgentEvent` variant?

The principle: **an event is durable if and only if replaying it changes the
projected `LoopContext` or `TurnTask` state.** Ephemeral events are observable
side-effects (streaming deltas, progress, warnings) that don't affect replay
correctness.

**Durable events** (replay must process):
- `SessionStarted` — sets session_id in loop state
- `SessionEnded` — sets end_reason
- `IterationStarted` — bumps iteration counter
- `LlmRequestStarted` — marks LLM turn boundary
- `LlmResponseComplete` — final LLM output, usage
- `ToolCallStarted` / `ToolCallCompleted` / `ToolCallError` / `ToolCallSkipped` —
  tool lifecycle, affects turn status
- `TurnStarted` / `TurnCompleted` / `TurnFailed` — turn lifecycle
- `SampleCompleted` — LLM sample boundary (replay uses this)
- `ToolCallIssued` / `ToolResultReceived` — tool lifecycle
- `ContextCompacted` — changes token budget state
- `Checkpoint` — recovery checkpoint
- `StateChange` — explicit state transition
- `RecoveryApplied` — recovery action (affects future behavior)
- `Status` — agent status change
- `SteeringReceived` — user interruption (affects turn flow)
- `GuardianConfirmationRequest` — blocks tool execution
- `SubagentSpawnBegin` / `SubagentSpawnEnd` / `SubagentComplete` — subagent
  lifecycle affects parent turn
- `Finish` — terminal output

**Ephemeral events** (replay can skip):
- `LlmStreamDelta` — streaming token, intermediate
- `LlmReasoningDelta` — streaming reasoning, intermediate
- `Thinking` — agent's internal monologue, observable but not state-changing
- `Progress` — progress notification, not state
- `Warning` — non-fatal warning, logged but doesn't change state
- `LoopWarning` — warning about loop detection (the loop detector state is
  separate; this is just the notification)
- `GuardianWarning` — guardian advisory, doesn't block
- `TokenBudgetNotice` / `TokenBudgetWarning` — budget notifications, the
  actual budget state is in `ContextCompacted`
- `IterationCompleted` — derivative of `IterationStarted` + turn completion
- `SessionInterrupted` — derivative of session lifecycle
- `HookError` — hook failure notification, doesn't change agent state
- `SelfReflection` — reflection output, observable but not state-changing
- `SubagentMessage` — intermediate subagent communication
- `SubagentEvent` — wrapped child event (the child's own durable events are
  in the child's log)

**Decision**: The classification above. Rationale: "durable = replaying this
event mutates `LoopContext` or `TurnTask`."

### Q3: Should `append_agent_event` signature change?

Current signature:
```rust
pub async fn append_agent_event<P>(
    session_path, aggregate, event_type, turn_id, iteration, payload,
) -> Result<PersistedEvent>
```

**Option A: Add `ephemeral: bool` parameter**
- Pro: Explicit, caller decides
- Con: Breaking change for all callers (~8 call sites in main_loop.rs)
- Con: Caller must know the classification (duplicates the method)

**Option B: Derive `ephemeral` from `event_type` string inside `append_agent_event`**
- Pro: No signature change, callers untouched
- Pro: Single source of truth (the lookup function)
- Con: String-based lookup is fragile (typo → silent misclassification)
- Con: Couples persistence layer to event type strings

**Option C: Change `append_agent_event` to accept `&AgentEvent` instead of `event_type: &str`**
- Pro: The method `is_durable()` is called internally
- Pro: Type-safe, no string lookup
- Con: Breaking change, larger refactor
- Con: `AgentEvent` is currently NOT what callers pass (they pass type + payload separately)

**Decision: Option B** — derive `ephemeral` from `event_type` inside the
append function via a lookup function `fn is_durable_event_type(type: &str) -> bool`.
This avoids touching all call sites and keeps the change small (~100 lines).
The lookup function is the persistence-layer projection of the `AgentEvent::is_durable()`
method. A test will assert that all `AgentEvent` variants' type strings are
covered by the lookup.

### Q4: Should `EventStore::append` signature change?

`EventStore::append` is the low-level persistence function in
`synthia-session`. It currently takes `source: EventSource`.

**Decision: Yes, add `ephemeral: bool` parameter.** This is the right layer to
persist the classification — `EventStore` is the single write path. Callers
that don't care about durability pass `false` (durable, the safe default).

### Q5: How does replay use the `ephemeral` field?

**Decision**: In `replay.rs`, `apply_event` and `reconstruct_turns` skip
events where `event.ephemeral == true` before entering the match. This:
- Makes the implicit `_ => {}` explicit
- Allows future ephemeral events to be added without touching replay.rs
- Enables a future optimization: skip deserialization of ephemeral event payloads
  entirely (read only the `ephemeral` flag from the JSON line)

### Q6: Backward compatibility with existing `events.jsonl` files?

**Decision**: `#[serde(default)]` on the `ephemeral` field, defaulting to
`false` (durable). This means:
- Old events without the field → treated as durable (safe: they were already
  being processed by replay)
- New events with `ephemeral: true` → skipped by replay
- No migration script needed

## Design Trade-offs

### Trade-off 1: Redundancy between method and field
The `AgentEvent::is_durable()` method and the `ephemeral` field on
`PersistedEvent` are redundant — the field is always derived from the method
at append time. This is intentional:
- The method is the source of truth for in-memory events
- The field is the persisted projection for readers that don't have the enum
- Keeping both in sync is a one-way flow (method → field), not bidirectional

**Mitigation**: A unit test asserts that for every `AgentEvent` variant, the
`is_durable()` result matches the `is_durable_event_type(type_name)` result.

### Trade-off 2: String-based lookup fragility
`is_durable_event_type(&str)` uses the event type string, which is fragile
(typos cause silent misclassification). 

**Mitigation**: 
1. The type strings are `const &str` constants in `persisted.rs`, not magic strings
2. A test iterates all `AgentEvent` variants, serializes them, extracts the
   `type` tag, and asserts the lookup matches `is_durable()`
3. clippy will catch unused variants

### Trade-off 3: Not filtering at read time
The `ephemeral` field enables future read-time filtering (skip JSON lines
where `ephemeral` is true without full deserialization), but this change does
NOT implement that optimization. It only skips after deserialization.

**Rationale**: YAGNI. The current bottleneck is not deserialization cost; it's
the implicit classification. Implementing partial JSON parsing for the
`ephemeral` flag is premature. The field is there for future use.

## What this change does NOT do

- Does NOT change which events are persisted (durability ≠ persistence; all
  events that are currently persisted remain persisted)
- Does NOT implement read-time JSON skipping (future optimization)
- Does NOT add a migration script for old events.jsonl (serde default handles it)
- Does NOT change the `AgentEvent` enum variants themselves
- Does NOT change the `AgentEventEmitter` channel (in-process, not persisted)
