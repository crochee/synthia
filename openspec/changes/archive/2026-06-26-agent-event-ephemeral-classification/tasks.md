## 1. Add `ephemeral` field to `PersistedEvent`

- [x] 1.1 Add `#[serde(default)] ephemeral: bool` field to `PersistedEvent` in `crates/synthia-session/src/store/events.rs`
- [x] 1.2 Update `EventStore::append` signature to accept `ephemeral: bool` parameter and persist it in the `PersistedEvent` record
- [x] 1.3 Verify `cargo check -p synthia-session` passes

## 2. Add durability classification to `AgentEvent`

- [x] 2.1 Add `pub fn is_durable(&self) -> bool` method to `AgentEvent` in `crates/synthia-agent/src/events/event_enum.rs` covering all ~30 variants per the D2 classification in design.md
- [x] 2.2 Add `pub fn is_durable_event_type(event_type: &str) -> bool` lookup function in `crates/synthia-agent/src/events/persisted.rs`, returning `true` for unknown types (safe default)
- [x] 2.3 Write a unit test that iterates all `AgentEvent` variants, serializes each to extract the `type` tag, and asserts `is_durable_event_type(tag) == is_durable()` for every variant
- [x] 2.4 Verify `cargo test -p synthia-agent` passes

## 3. Wire classification into persistence path

- [x] 3.1 Update `append_agent_event` in `crates/synthia-agent/src/events/persisted.rs` to derive `ephemeral` via `is_durable_event_type(event_type)` and pass it to `EventStore::append`
- [x] 3.2 Update existing tests in `persisted.rs` that call `EventStore::append` directly to pass the `ephemeral` parameter
- [x] 3.3 Verify `cargo test -p synthia-agent --lib` passes

## 4. Update replay harness to skip ephemeral events

- [x] 4.1 In `crates/synthia-agent/src/replay.rs`, add `if event.ephemeral { return; }` guard at the start of `apply_event` before the match
- [x] 4.2 In `replay.rs`, add the same guard at the start of `apply_turn_event` in `reconstruct_turns`
- [x] 4.3 Add a test in `replay.rs` that writes both durable and ephemeral events and confirms only durable events affect the projected state
- [x] 4.4 Add a test in `replay.rs` that deserializes old-format JSONL (without `ephemeral` field) and confirms all events are treated as durable

## 5. Backward compatibility and integration tests

- [x] 5.1 Add a test that constructs a `PersistedEvent` JSON line without the `ephemeral` field, deserializes it, and asserts `ephemeral == false`
- [x] 5.2 Add a test that appends an ephemeral event and confirms the persisted JSONL line contains `"ephemeral":true`
- [x] 5.3 Run `cargo +nightly fmt --all` to format all modified files
- [x] 5.4 Run `cargo clippy --all-targets --all-features --tests --all` and fix all warnings
- [x] 5.5 Run `cargo test --workspace` to ensure no regressions
