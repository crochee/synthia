# Server V2 Session Controller Implementation Plan

> **For agentic workers:** Implement this plan task-by-task. Each task should result in a compiling, testable increment.

**Goal:** Transform `synthia-server` into a multi-client-capable backend where sessions have a single execution controller, REST-based steering/cancel, user isolation, cursor-paginated endpoints, and persistent event replay.

**Architecture:** A per-session `SessionController` serializes `Prompt`/`Steer`/`Cancel`/`Shutdown` operations over an `mpsc` channel, spawns at most one `Agent::run_stream`, persists events to `events.jsonl`, and broadcasts them via SSE. V2 REST endpoints follow `api-design` conventions with envelope responses and cursor pagination. V1 routes remain compatible but deprecated.

**Tech Stack:** Rust, Axum, `tokio::sync`, `tokio_util::sync::CancellationToken`, existing `synthia-session`, `synthia-agent`, `synthia-server`.

---

## Task 1: Extend session persistence for events

**Files to touch:**
- `crates/synthia-session/src/store/types.rs`
- `crates/synthia-session/src/store/mod.rs` (or new `crates/synthia-session/src/store/events.rs`)

**Steps:**
- [ ] **Step 1.1:** Add `title: Option<String>` and `controller_version: u32` to `SessionMetadata` with `#[serde(default)]`.
- [ ] **Step 1.2:** Define `PersistedEvent` struct:
  ```rust
  pub struct PersistedEvent {
      pub seq: u64,
      pub aggregate: String,
      #[serde(rename = "type")]
      pub event_type: String,
      pub ts: DateTime<Utc>,
      pub source: EventSource,
      pub payload: serde_json::Value,
  }
  ```
- [ ] **Step 1.3:** Implement `EventStore::append(session_path, AgentEvent) -> Result<PersistedEvent>`:
  - Read current max seq from `events.jsonl` if it exists (or from a lightweight counter).
  - Append one JSON line.
- [ ] **Step 1.4:** Implement `EventStore::read_from(session_path, last_seq, limit) -> Result<Vec<PersistedEvent>>`.
- [ ] **Step 1.5:** Write unit tests for append/read and backward compatibility (missing `events.jsonl` starts at seq 1).

**Verification:** `cargo test -p synthia-session` passes.

---

## Task 2: User-isolated SessionManager operations

**Files to touch:**
- `crates/synthia-session/src/manager/core.rs`
- `crates/synthia-session/src/manager/mod.rs` (if facade)

**Steps:**
- [ ] **Step 2.1:** Add `list_for_user(&self, user_id: &str) -> Vec<SessionSummary>`.
- [ ] **Step 2.2:** Add `get_for_user(&self, user_id: &str, session_id: &str) -> Result<Session>`.
- [ ] **Step 2.3:** Add `delete_for_user(&self, user_id: &str, session_id: &str) -> Result<()>`.
- [ ] **Step 2.4:** Ensure all three return `NotFound` for cross-user access without leaking existence.
- [ ] **Step 2.5:** Add unit tests using temp directories.

**Verification:** `cargo test -p synthia-session` passes.

---

## Task 3: Build SessionController

**Files to touch:**
- `crates/synthia-server/src/session/controller.rs` (new)
- `crates/synthia-server/src/session/mod.rs` (new)
- `crates/synthia-server/src/state/app_state.rs`

**Steps:**
- [ ] **Step 3.1:** Define `SessionOp` enum.
- [ ] **Step 3.2:** Define `SessionController` struct holding `session_id`, `user_id`, `state`, `op_rx`, `cancel_token`, `broadcaster`, `input_queue`, `current_run`.
- [ ] **Step 3.3:** Implement `SessionController::new(...)` and a `spawn(...)` constructor that returns `(Arc<Self>, mpsc::Sender<SessionOp>)`.
- [ ] **Step 3.4:** Implement `run()` loop:
  - `Prompt`/`Steer` → append to `session_input.jsonl` → `maybe_start_run()`.
  - `Cancel` → `cancel_token.cancel()`.
  - `Shutdown` → break.
- [ ] **Step 3.5:** Implement `maybe_start_run()`:
  - If `state == Idle` and queue has unconsumed items, spawn `Agent::run_stream` with `session_input_queue` set.
  - Forward events: persist first, then broadcast.
- [ ] **Step 3.6:** Implement idle timeout: when no run and no SSE subscribers for `IDLE_TIMEOUT`, send `Shutdown`.

**Verification:** Add unit tests for concurrent prompts not spawning multiple runs; `cargo test -p synthia-server`.

---

## Task 4: Shared API utilities

**Files to touch:**
- `crates/synthia-server/src/api/mod.rs` (new)
- `crates/synthia-server/src/api/error.rs` (new)
- `crates/synthia-server/src/api/pagination.rs` (new)
- `crates/synthia-server/src/api/envelope.rs` (new)

**Steps:**
- [ ] **Step 4.1:** Define `ApiError` with `code`, `message`, `details`; implement `IntoResponse`.
- [ ] **Step 4.2:** Define `Cursor<T>` with base64 URL-safe JSON encoding/decoding.
- [ ] **Step 4.3:** Define `PaginatedResponse<T>` with `data`, `meta { has_next, next_cursor }`, `links { self, next }`.
- [ ] **Step 4.4:** Add request validation helpers returning `422 validation_error`.
- [ ] **Step 4.5:** Unit-test cursor round-trip and invalid cursor handling.

**Verification:** `cargo test -p synthia-server` passes.

---

## Task 5: Implement V2 routes

**Files to touch:**
- `crates/synthia-server/src/server/router.rs`
- `crates/synthia-server/src/routes/v2/sessions.rs` (extend)
- `crates/synthia-server/src/routes/v2/prompts.rs` (new)
- `crates/synthia-server/src/routes/v2/steering.rs` (new)
- `crates/synthia-server/src/routes/v2/cancel.rs` (new)
- `crates/synthia-server/src/routes/v2/events.rs` (new)
- `crates/synthia-server/src/routes/v2/messages.rs` (new)

**Steps:**
- [ ] **Step 5.1:** Mount `/api/v2/sessions` routes.
- [ ] **Step 5.2:** Implement `POST /api/v2/sessions` returning `201 Created` + `Location`.
- [ ] **Step 5.3:** Implement `GET /api/v2/sessions` with cursor pagination.
- [ ] **Step 5.4:** Implement `GET /api/v2/sessions/{id}` and `DELETE /api/v2/sessions/{id}` with user checks.
- [ ] **Step 5.5:** Implement `POST /api/v2/sessions/{id}/prompts` returning `202 Accepted`.
- [ ] **Step 5.6:** Implement `POST /api/v2/sessions/{id}/steering` with default priority 255.
- [ ] **Step 5.7:** Implement `POST /api/v2/sessions/{id}/cancel`.
- [ ] **Step 5.8:** Implement `GET /api/v2/sessions/{id}/events` SSE with `last_seq` replay and `SyncCaughtUp`.
- [ ] **Step 5.9:** Implement `GET /api/v2/sessions/{id}/messages` with cursor pagination by seq.

**Verification:** `cargo check -p synthia-server` passes.

---

## Task 6: Refactor WebSocket and deprecate V1

**Files to touch:**
- `crates/synthia-server/src/routes/ws.rs`
- `crates/synthia-server/src/routes/chat.rs`
- `crates/synthia-server/src/server/router.rs`

**Steps:**
- [ ] **Step 6.1:** Change WebSocket handler to subscribe to existing broadcaster only; remove run-spawning logic.
- [ ] **Step 6.2:** Add `Deprecation: true` header to all V1 route responses.
- [ ] **Step 6.3:** Ensure `GET /api/v1/sessions/{id}/stream-sse` still receives events from V2 controller.
- [ ] **Step 6.4:** Update `POST /api/v1/chat` to use `create_with_user(user_id)`.

**Verification:** Existing V1 tests pass; no compile errors.

---

## Task 7: Integration tests

**Files to touch:**
- `crates/synthia-server/tests/v2_session_controller.rs` (new)

**Steps:**
- [ ] **Step 7.1:** Test multi-client SSE observation: client A sends prompt, clients B and C receive identical events.
- [ ] **Step 7.2:** Test steering/cancel via HTTP: prompt starts run, steering is consumed, cancel terminates run.
- [ ] **Step 7.3:** Test user isolation: user A cannot access user B's session (404).
- [ ] **Step 7.4:** Test SSE replay: connect at seq N, disconnect, reconnect with `last_seq=N`, receive missed events.
- [ ] **Step 7.5:** Test cursor pagination for sessions and messages.
- [ ] **Step 7.6:** Run `cargo clippy --all-targets --all-features --tests --all` and fix warnings.
- [ ] **Step 7.7:** Run `cargo +nightly fmt --all`.

**Verification:** All tests pass; clippy clean; fmt clean.

---

## Rollback Procedure

If critical issues arise after merge:
1. Revert `synthia-server` changes.
2. Remove `events.jsonl` files (optional; they are ignored by old code).
3. V1 routes continue to work with pre-change behavior.
