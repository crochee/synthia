## Context

The agent-turn-events-phase1 change (merged 2026-06-25, commit 3d4808e)
introduced a JSONL-based event persistence layer and a replay harness
(`crates/synthia-agent/src/replay.rs`). The harness reads
`{session_path}/events.jsonl` line-by-line and projects `LoopContext` +
`Vec<TurnTask>` state for session reconstruction.

**Current state**: The replay harness has no explicit notion of which events
are *durable* (state-changing, must be replayed) vs *ephemeral* (observable
but non-state-changing, can be skipped). The `apply_event` function in
`replay.rs` implicitly skips unknown event types via `_ => {}`. The
classification lives in replay.rs's match arms, not in the event type itself.

**Constraints**:
- `AgentEvent` enum has ~30 variants (file: `events/event_enum.rs`)
- `PersistedEvent` struct has 6 fields: seq, aggregate, event_type, ts, source, payload
- `append_agent_event` writes events via `EventStore::append` (file: `persisted.rs`)
- Existing `events.jsonl` files must remain readable (no migration script)
- The change should be ~100 lines per project memory estimate

**Stakeholders**: replay harness (primary consumer), telemetry/UI (secondary),
tests (tertiary).

## Goals / Non-Goals

**Goals:**
- Make durable/ephemeral classification explicit at both the `AgentEvent` type
  layer and the `PersistedEvent` persistence layer
- Allow replay to skip ephemeral events by checking a persisted field (no
  payload parsing required for the skip decision)
- Keep backward compatibility: old `events.jsonl` files (without the field)
  deserialize as durable (the safe default)
- Single source of truth: `AgentEvent::is_durable()` method → drives the
  `ephemeral` field on `PersistedEvent` at append time

**Non-Goals:**
- NOT implementing read-time JSON skipping (partial parse of the `ephemeral`
  flag before full deserialization) — future optimization, YAGNI
- NOT changing which events are persisted (all currently-persisted events
  remain persisted; durability ≠ persistence policy)
- NOT changing the `AgentEvent` enum variants themselves
- NOT changing the `AgentEventEmitter` in-process channel
- NOT adding a migration script for old events.jsonl (serde default handles it)

## Decisions

### D1: Two-layer classification (method + field)

- **选择**: Add `fn is_durable(&self) -> bool` to `AgentEvent` (source of
  truth) AND `ephemeral: bool` field to `PersistedEvent` (persisted
  projection, derived from the method at append time).
- **理由**: The method serves in-memory consumers (agent loop, tests); the
  field serves external readers (replay, telemetry) that only see
  `PersistedEvent`. The field is a one-way projection of the method, not a
  bidirectional sync. This is the opencode pattern: `DurableDefinitions`
  (type union) + persisted `ephemeral` flag.
- **已考虑 alternative**:
  - Method only: rejected — replay still deserializes every line and can't
    skip without the enum.
  - Field only: rejected — in-memory consumers (agent loop) have no way to
    classify without persisting first.

### D2: Durability classification per variant

- **选择**: "Durable = replaying this event mutates `LoopContext` or
  `TurnTask`." Ephemeral = observable side-effect only.
- **Durable**: SessionStarted, SessionEnded, IterationStarted,
  LlmRequestStarted, LlmResponseComplete, ToolCallStarted,
  ToolCallCompleted, ToolCallError, ToolCallSkipped, TurnStarted,
  TurnCompleted, TurnFailed, SampleCompleted, ToolCallIssued,
  ToolResultReceived, ContextCompacted, Checkpoint, StateChange,
  RecoveryApplied, Status, SteeringReceived, GuardianConfirmationRequest,
  SubagentSpawnBegin, SubagentSpawnEnd, SubagentComplete, Finish.
- **Ephemeral**: LlmStreamDelta, LlmReasoningDelta, Thinking, Progress,
  Warning, LoopWarning, GuardianWarning, TokenBudgetNotice,
  TokenBudgetWarning, IterationCompleted, SessionInterrupted, HookError,
  SelfReflection, SubagentMessage, SubagentEvent.
- **理由**: Matches the existing implicit classification in `replay.rs`
  `apply_event` (which already handles durable variants and skips the rest
  via `_ => {}`). Making it explicit prevents future regressions.
- **已考虑 alternative**: Classify `SubagentEvent` as durable (it wraps a
  child event). Rejected — the child's own durable events are in the child's
  `events.jsonl`; the parent only sees the wrapper for observability.

### D3: Derive `ephemeral` from event_type string inside append path

- **选择**: Add `fn is_durable_event_type(event_type: &str) -> bool` in
  `persisted.rs`. `append_agent_event` calls this internally; no signature
  change to `append_agent_event`.
- **理由**: Avoids touching ~8 call sites in `main_loop.rs`. Keeps the
  change within ~100 lines. The lookup is the persistence-layer projection
  of `AgentEvent::is_durable()`.
- **已考虑 alternative**:
  - Add `ephemeral: bool` parameter to `append_agent_event`: rejected —
    duplicates classification at every call site.
  - Change `append_agent_event` to accept `&AgentEvent`: rejected — larger
    refactor, callers pass (type, payload) separately today.

### D4: `EventStore::append` gains `ephemeral: bool` parameter

- **选择**: Add `ephemeral: bool` to `EventStore::append` signature.
  `append_agent_event` passes the derived value. Other callers (non-agent)
  pass `false` (durable, safe default).
- **理由**: `EventStore` is the single write path; persisting the
  classification here ensures every persisted record carries it.
- **已考虑 alternative**: Add a separate `EventStore::append_ephemeral`:
  rejected — two write paths, more code, no benefit.

### D5: Backward compatibility via `#[serde(default)]`

- **选择**: `#[serde(default)]` on `ephemeral: bool`, defaulting to `false`
  (durable). Old events without the field → durable (they were already
  being processed by replay).
- **理由**: No migration script needed. Old files remain readable. The
  default is the safe direction (durable = processed = no behavior change).

### D6: Replay skips ephemeral events before match

- **选择**: In `replay.rs`, `apply_event` and `reconstruct_turns` check
  `if event.ephemeral { return; }` before the match arms.
- **理由**: Makes the implicit `_ => {}` explicit. Future ephemeral events
  are automatically skipped without touching replay.rs. The match arms for
  durable events remain as documentation of what replay processes.
- **已考虑 alternative**: Remove the `_ => {}` arm entirely. Rejected —
  keeping it as a no-op documents that unknown event types are tolerated
  (forward compatibility with new event types in old replay code).

## Risks / Trade-offs

- [Risk] String-based lookup `is_durable_event_type(&str)` is fragile (typo →
  silent misclassification) → Mitigation: type strings are `const &str`
  constants; a test iterates all `AgentEvent` variants, serializes them,
  extracts the `type` tag, and asserts the lookup matches `is_durable()`.
- [Risk] Method and field drift (someone adds a variant but forgets to update
  the lookup) → Mitigation: the same test above catches this; clippy catches
  unused enum variants.
- [Trade-off] Redundancy between `AgentEvent::is_durable()` and the `ephemeral`
  field → Accept: the field is a one-way projection (method → field), not
  bidirectional. External readers need the field without the enum.
- [Trade-off] Not filtering at read time (still deserializes every line)
  → Accept: YAGNI. The current bottleneck is classification clarity, not
  deserialization cost. The field enables future partial-JSON optimization.
- [Risk] `EventStore::append` signature change breaks external callers →
  Mitigation: only `append_agent_event` and tests call it; both updated in
  the same change.

## Migration Plan

1. Add `ephemeral: bool` field to `PersistedEvent` with `#[serde(default)]`.
2. Add `is_durable()` to `AgentEvent` and `is_durable_event_type()` to
   `persisted.rs`.
3. Update `EventStore::append` signature; update `append_agent_event` to
   derive and pass `ephemeral`.
4. Update `replay.rs` to skip ephemeral events.
5. Add tests: (a) classification consistency, (b) backward compat with old
   JSONL, (c) replay skips ephemeral.
6. Run `cargo +nightly fmt --all` and `cargo clippy --all-targets
   --all-features --tests --all`.

**Rollback**: revert the commit. Old code reading new-format JSONL ignores
the `ephemeral` field (serde skips unknown fields by default). New code
reading old-format JSONL treats all events as durable (safe default).

**Acceptance criteria**:
- `cargo test` passes
- A test with old-format JSONL (no `ephemeral` field) replays correctly
- A test with ephemeral events confirms they are skipped by replay
- `AgentEvent::is_durable()` matches `is_durable_event_type()` for all variants

## Open Questions

None — all design questions resolved in brainstorm.md.
