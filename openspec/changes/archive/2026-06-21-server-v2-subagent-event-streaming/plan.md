# Subagent Event Streaming Implementation Plan

> **For agentic workers:** Implement this plan task-by-task. Each task should result in a compiling, testable increment.

**Goal:** Make subagents observable as independent persistent sessions by bridging their event streams into the parent session and exposing them through the V2 API.

**Architecture:** A `SubagentSessionFactory` injected into `AgentRunConfig` lets `AgentTool` create real child sessions. Each child gets its own `SessionController`; child events are wrapped as `AgentEvent::SubagentEvent` and forwarded to the parent controller's event channel, where they are persisted and broadcast. A new `GET /api/v2/sessions/{id}/subagents` endpoint lists child sessions.

**Tech Stack:** Rust, Axum, `tokio::sync`, existing `synthia-session`, `synthia-agent`, `synthia-server`.

---

## Task 1: Extend session data model for parent_id

**Files to touch:**
- `crates/synthia-session/src/store/types.rs`
- `crates/synthia-session/src/types/session.rs`
- `crates/synthia-session/src/manager/types.rs`
- `crates/synthia-session/src/store/metadata.rs`

**Steps:**
- [ ] **Step 1.1:** Add `parent_id: Option<String>` to `SessionMetadata` with `#[serde(default)]`.
- [ ] **Step 1.2:** Add `parent_id: Option<String>` to `types::Session` and `SessionSummary`.
- [ ] **Step 1.3:** Add `parent_id: Option<String>` to `SessionFilter`.
- [ ] **Step 1.4:** Update metadata save/load paths to handle the new field.
- [ ] **Step 1.5:** Write unit test `test_metadata_parent_id_default`.

**Verification:** `cargo test -p synthia-session` passes.

---

## Task 2: Add SessionManager child operations

**Files to touch:**
- `crates/synthia-session/src/manager/core.rs`
- `crates/synthia-session/src/store/mod.rs`

**Steps:**
- [ ] **Step 2.1:** Implement `create_child(&self, user_id, parent_session_id, id?) -> Result<Session>` that sets `parent_id`.
- [ ] **Step 2.2:** Implement `list_children(&self, user_id, parent_session_id) -> Result<Vec<SessionMetadata>>`.
- [ ] **Step 2.3:** Ensure user isolation: reject cross-user parent/child access.
- [ ] **Step 2.4:** Add unit tests for create_child and list_children.

**Verification:** `cargo test -p synthia-session` passes.

---

## Task 3: Define SubagentSessionFactory and wire AgentRunConfig

**Files to touch:**
- `crates/synthia-agent/src/subagent/factory.rs` (new)
- `crates/synthia-agent/src/config/agent_config/run_config.rs`
- `crates/synthia-agent/src/config/agent_config/run_config_builder.rs`
- `crates/synthia-agent/src/lib.rs`

**Steps:**
- [ ] **Step 3.1:** Define `SubagentSessionFactory` trait and `ChildSessionHandle`:
  ```rust
  #[async_trait]
  pub trait SubagentSessionFactory: Send + Sync {
      async fn create_child(
          &self,
          user_id: String,
          parent_session_id: String,
          maybe_id: Option<String>,
      ) -> Result<ChildSessionHandle, SubagentSessionError>;
  }

  pub struct ChildSessionHandle {
      pub session_id: String,
      pub user_id: String,
      pub parent_event_sender: mpsc::Sender<AgentEvent>,
  }
  ```
- [ ] **Step 3.2:** Add `subagent_session_factory: Option<Arc<dyn SubagentSessionFactory>>` to `AgentRunConfig` with `#[serde(skip_serializing, skip_deserializing)]`.
- [ ] **Step 3.3:** Add setter on `AgentRunConfigBuilder`.
- [ ] **Step 3.4:** Update all production `AgentRunConfig` literals to pass `None`.

**Verification:** `cargo check -p synthia-agent` passes.

---

## Task 4: Implement server-side SubagentSessionFactory

**Files to touch:**
- `crates/synthia-server/src/state/subagent_factory.rs` (new)
- `crates/synthia-server/src/state/mod.rs`
- `crates/synthia-server/src/state/app_state.rs`

**Steps:**
- [ ] **Step 4.1:** Create `AppStateSubagentFactory` implementing `SubagentSessionFactory`.
- [ ] **Step 4.2:** In `create_child`, call `SessionManager::create_child` and `get_or_create_session_controller` for the child.
- [ ] **Step 4.3:** Return `ChildSessionHandle` with the parent controller's `event_tx`.
- [ ] **Step 4.4:** Register the factory in `AppState` and set it on `AgentRunConfig` when building configs.

**Verification:** `cargo check -p synthia-server` passes.

---

## Task 5: Refactor AgentTool and run_subagent to use real child sessions

**Files to touch:**
- `crates/synthia-agent/src/tools/agent_tools/agent_tool.rs`
- `crates/synthia-agent/src/subagent/runner.rs`
- `crates/synthia-agent/src/tools/agent_tools/team.rs`

**Steps:**
- [ ] **Step 5.1:** Update `AgentTool::call` to require `subagent_session_factory`.
- [ ] **Step 5.2:** Create child session via factory, enqueue the task description as a prompt via child controller.
- [ ] **Step 5.3:** Await child completion and return `AgentResult`.
- [ ] **Step 5.4:** Remove or deprecate placeholder `run_subagent` and `SubagentManager` stubs no longer needed.
- [ ] **Step 5.5:** Update existing subagent integration tests.

**Verification:** `cargo test -p synthia-agent` passes.

---

## Task 6: Add event forwarding channel to SessionController

**Files to touch:**
- `crates/synthia-server/src/session/controller.rs`

**Steps:**
- [ ] **Step 6.1:** Add `event_tx: mpsc::Sender<AgentEvent>` and `event_rx: mpsc::Receiver<AgentEvent>` to `SessionController`.
- [ ] **Step 6.2:** Update `run_controller_loop` to `tokio::select!` on `op_rx` and `event_rx`.
- [ ] **Step 6.3:** For forwarded events, call `persist_and_broadcast`.
- [ ] **Step 6.4:** When creating a child controller, pass `parent_event_sender` (parent's `event_tx`).
- [ ] **Step 6.5:** In child controller's `persist_and_broadcast`, after child broadcast, send `AgentEvent::SubagentEvent { child_session_id, event }` to `parent_event_sender`; handle closed channel gracefully.
- [ ] **Step 6.6:** Add unit tests for forwarding and closed-parent behavior.

**Verification:** `cargo test -p synthia-server` passes.

---

## Task 7: Add AgentEvent::SubagentEvent and SSE mapping

**Files to touch:**
- `crates/synthia-agent/src/events/event_enum.rs`
- `crates/synthia-server/src/sse.rs`

**Steps:**
- [ ] **Step 7.1:** Add `SubagentEvent { child_session_id: String, event: Box<AgentEvent> }` variant.
- [ ] **Step 7.2:** Update serialization tests.
- [ ] **Step 7.3:** Map `SubagentEvent` to SSE event name `subagent_event` in `agent_event_to_sse`.
- [ ] **Step 7.4:** Add SSE serialization tests.

**Verification:** `cargo test -p synthia-agent -p synthia-server` passes.

---

## Task 8: Implement GET /api/v2/sessions/{id}/subagents

**Files to touch:**
- `crates/synthia-server/src/routes/v2/subagents.rs` (new)
- `crates/synthia-server/src/routes/v2/mod.rs`
- `crates/synthia-server/src/server/router.rs`
- `crates/synthia-server/src/routes/v2/models.rs`

**Steps:**
- [ ] **Step 8.1:** Add `GET /api/v2/sessions/{id}/subagents` route.
- [ ] **Step 8.2:** Verify parent session belongs to caller; return 404 otherwise.
- [ ] **Step 8.3:** Call `SessionManager::list_children` and return paginated `SessionSummary`.
- [ ] **Step 8.4:** Include `parent_id` in `SessionSummary` responses across V2 endpoints.
- [ ] **Step 8.5:** Add integration tests.

**Verification:** `cargo test -p synthia-server` passes.

---

## Task 9: Integration tests and cleanup

**Files to touch:**
- `crates/synthia-server/tests/subagent_event_streaming.rs` (new)
- `crates/synthia-agent/tests/subagent_integration_test.rs`

**Steps:**
- [ ] **Step 9.1:** Test parent `/events` receives `SubagentEvent` wrappers when a subagent runs.
- [ ] **Step 9.2:** Test child `/events` contains raw child events.
- [ ] **Step 9.3:** Test parent event replay includes subagent history.
- [ ] **Step 9.4:** Test multi-client observation of subagent progress.
- [ ] **Step 9.5:** Run `cargo +nightly fmt --all`.
- [ ] **Step 9.6:** Run `cargo clippy --all-targets --all-features --tests --all` and fix warnings.
- [ ] **Step 9.7:** Run `cargo test -p synthia-session -p synthia-server -p synthia-agent --all-features`.

**Verification:** All tests pass; clippy clean; fmt clean.

---

## Rollback Procedure

1. Revert code changes.
2. Session metadata with `parent_id` remains readable due to `#[serde(default)]`.
3. Child `events.jsonl` files are ignored by old code but remain on disk.
