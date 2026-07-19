## 1. Data model and SessionManager extensions

- [x] 1.1 Add `parent_id: Option<String>` to `SessionMetadata` with `#[serde(default)]` and update save/load paths.
- [x] 1.2 Add `parent_id` to canonical `types::Session`, `SessionSummary`, and `SessionFilter`.
- [x] 1.3 Add `SessionManager::create_child(user_id, parent_session_id, id?)` and unit tests.
- [x] 1.4 Add `SessionManager::list_children(user_id, parent_session_id)` with user isolation and unit tests.
- [x] 1.5 Add backward-compat test: metadata without `parent_id` loads as `None`.

## 2. SubagentSessionFactory and AgentRunConfig wiring

- [x] 2.1 Define `SubagentSessionFactory` trait and `ChildSessionHandle` in `synthia-agent`.
- [x] 2.2 Add `subagent_session_factory: Option<Arc<dyn SubagentSessionFactory>>` to `AgentRunConfig` with serde skip.
- [x] 2.3 Update all production `AgentRunConfig` struct literals to pass `None`.
- [x] 2.4 Implement `SubagentSessionFactory` in `synthia-server` backed by `AppState`.

## 3. AgentTool and run_subagent refactoring

- [x] 3.1 Replace placeholder `run_subagent` with real child session creation via factory.
- [x] 3.2 Update `AgentTool::call` to create child session, enqueue child prompt, and await completion.
- [x] 3.3 Preserve existing depth/concurrency limits and error handling.
- [x] 3.4 Update `SubagentManager` to remove or deprecate stubs superseded by real sessions.

## 4. SessionController event forwarding

- [x] 4.1 Add forwarded-event channel (`event_tx`/`event_rx`) to `SessionController`.
- [x] 4.2 Update controller run loop to select on `op_rx` and `event_rx`.
- [x] 4.3 Forward child events as `AgentEvent::SubagentEvent` to parent controller's `event_tx`.
- [x] 4.4 Ensure best-effort forwarding: log and continue on closed parent channel.
- [x] 4.5 Add unit tests for controller forwarding and closed-channel behavior.

## 5. AgentEvent and SSE mapping

- [x] 5.1 Add `AgentEvent::SubagentEvent { child_session_id, event }` variant.
- [x] 5.2 Update serialization tests for the new variant.
- [x] 5.3 Map `SubagentEvent` to SSE event name `subagent_event`.
- [x] 5.4 Add SSE serialization tests.

## 6. V2 API and routes

- [x] 6.1 Add `parent_id` to V2 session response models.
- [x] 6.2 Implement `GET /api/v2/sessions/{id}/subagents` with user isolation and cursor pagination.
- [x] 6.3 Register the new route in the V2 router.
- [x] 6.4 Add integration tests for the subagents endpoint.

## 7. Integration and verification

- [x] 7.1 Write integration test: parent prompt spawns child; parent `/events` receives `SubagentEvent` wrappers.
- [x] 7.2 Write integration test: child `/events` contains raw child events.
- [x] 7.3 Write integration test: parent event replay includes subagent history.
- [x] 7.4 Write integration test: multi-client observation of subagent progress.
- [x] 7.5 Run `cargo +nightly fmt --all`.
- [x] 7.6 Run `cargo clippy --all-targets --all-features --tests --all` and fix warnings.
- [x] 7.7 Run `cargo test -p synthia-session -p synthia-server -p synthia-agent --all-features`.
