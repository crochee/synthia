# AgentEvent Ephemeral Classification Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development
> to implement this plan task-by-task.

**Goal:** Add explicit durable/ephemeral classification to `AgentEvent` and
`PersistedEvent` so the replay harness can skip ephemeral events without
parsing their payload.

**Architecture:** Two-layer classification: `AgentEvent::is_durable()` method
(source of truth) + `ephemeral: bool` field on `PersistedEvent` (persisted
projection). The field is derived from the method via
`is_durable_event_type(&str)` lookup at append time. Backward compatible via
`#[serde(default)]`.

**Tech Stack:** Rust, serde, tokio, tempfile (tests)

---

## Task 1: Add `ephemeral` field to `PersistedEvent`

- [ ] **Step 1:** Open `crates/synthia-session/src/store/events.rs`. Add
  `#[serde(default)] pub ephemeral: bool` field to the `PersistedEvent`
  struct, after `source` and before `payload`.
- [ ] **Step 2:** Update `EventStore::append` signature to add
  `ephemeral: bool` parameter. Set the field in the constructed
  `PersistedEvent`.
- [ ] **Step 3:** Run `cargo check -p synthia-session` to verify it compiles.
  Fix any callers of `EventStore::append` in the same crate (tests).
- [ ] **Commit point:** `feat(session): add ephemeral field to PersistedEvent`

## Task 2: Add durability classification to `AgentEvent`

- [ ] **Step 1:** Open `crates/synthia-agent/src/events/event_enum.rs`. Add
  `pub fn is_durable(&self) -> bool` method to `impl AgentEvent`. Match all
  ~30 variants per the D2 classification in design.md. Durable variants
  return `true`; ephemeral variants return `false`.
- [ ] **Step 2:** Open `crates/synthia-agent/src/events/persisted.rs`. Add
  `pub fn is_durable_event_type(event_type: &str) -> bool` that matches
  the known event type constants (`TURN_STARTED`, etc.) and returns `true`
  for unknown types (safe default).
- [ ] **Step 3:** In the same file, add a test module (or extend existing)
  with a test that iterates all `AgentEvent` variants, serializes each
  with `serde_json::to_value`, extracts the `"type"` tag, and asserts
  `is_durable_event_type(tag) == variant.is_durable()`.
- [ ] **Step 4:** Run `cargo test -p synthia-agent --lib` to verify the
  classification consistency test passes.
- [ ] **Commit point:** `feat(agent): add is_durable classification to AgentEvent`

## Task 3: Wire classification into persistence path

- [ ] **Step 1:** In `crates/synthia-agent/src/events/persisted.rs`, update
  `append_agent_event` to call `is_durable_event_type(&event_type)` and
  pass `!result` as the `ephemeral` parameter to `EventStore::append`.
- [ ] **Step 2:** Update existing tests in `persisted.rs` that call
  `EventStore::append` directly to pass the `ephemeral` parameter
  (use `false` for durable test events).
- [ ] **Step 3:** Run `cargo test -p synthia-agent --lib` to verify all
  existing tests still pass with the new signature.
- [ ] **Commit point:** `feat(agent): wire ephemeral classification into append path`

## Task 4: Update replay harness to skip ephemeral events

- [ ] **Step 1:** Open `crates/synthia-agent/src/replay.rs`. In
  `apply_event`, add `if event.ephemeral { return; }` as the first line
  before the `match event.event_type.as_str()` block.
- [ ] **Step 2:** In `replay.rs`, add the same guard
  (`if event.ephemeral { return; }`) at the start of `apply_turn_event`
  in `reconstruct_turns`.
- [ ] **Step 3:** Add a test `replay_skips_ephemeral_events` that writes
  a `TurnStarted` (durable) + `LlmStreamDelta` (ephemeral, manually
  crafted `PersistedEvent` with `ephemeral: true`) + `TurnCompleted`
  (durable), and confirms the projected state matches a replay without
  the ephemeral event.
- [ ] **Step 4:** Add a test `replay_old_format_jsonl_without_ephemeral`
  that constructs JSON lines without the `ephemeral` field (simulating
  old-format files) and confirms all events are treated as durable.
- [ ] **Step 5:** Run `cargo test -p synthia-agent --lib` to verify
  replay tests pass.
- [ ] **Commit point:** `feat(agent): replay skips ephemeral events`

## Task 5: Backward compatibility and integration verification

- [ ] **Step 1:** Add a test `persisted_event_without_ephemeral_defaults_durable`
  in `synthia-session` that deserializes a JSON line missing the `ephemeral`
  field and asserts `ephemeral == false`.
- [ ] **Step 2:** Add a test `append_ephemeral_event_persists_flag` in
  `synthia-agent` that appends an event with a known ephemeral type and
  confirms the persisted JSONL line contains `"ephemeral":true`.
- [ ] **Step 3:** Run `cargo +nightly fmt --all` to format all modified files.
- [ ] **Step 4:** Run `cargo clippy --all-targets --all-features --tests --all`
  and fix all warnings.
- [ ] **Step 5:** Run `cargo test --workspace` to ensure no regressions.
- [ ] **Commit point:** `test(agent): ephemeral classification integration tests`
