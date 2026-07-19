## 1. Data model and persistence

- [x] 1.1 Add `title` and `controller_version` fields to `SessionMetadata` with `serde(default)` for backward compatibility.
- [x] 1.2 Define `PersistedEvent` envelope struct with `seq`, `aggregate`, `type`, `ts`, `source`, `payload`.
- [x] 1.3 Implement `EventStore::append(session_path, event)` that appends to `events.jsonl` with monotonic seq.
- [x] 1.4 Implement `EventStore::read_from(session_path, last_seq, limit)` for SSE replay.
- [x] 1.5 Add fallback logic: sessions without `events.jsonl` start seq from 1.

## 2. SessionManager user isolation

- [x] 2.1 Add `SessionManager::list_for_user(user_id)` returning filtered sessions.
- [x] 2.2 Add `SessionManager::get_for_user(user_id, session_id)` returning session or `NotFound`.
- [x] 2.3 Add `SessionManager::delete_for_user(user_id, session_id)` enforcing ownership.
- [x] 2.4 Change server `POST /api/v1/sessions` and `POST /api/v1/chat` to call `create_with_user(user_id)`.
- [x] 2.5 Update `AppState` session index to be keyed by `(user_id, session_id)`.

## 3. SessionController implementation

- [x] 3.1 Define `SessionOp` enum (`Prompt`, `Steer`, `Cancel`, `Shutdown`).
- [x] 3.2 Implement `SessionController` struct with `state`, `op_rx`, `cancel_token`, `broadcaster`, `input_queue`, `current_run`.
- [x] 3.3 Implement `SessionController::run()` loop handling all ops and transitioning state.
- [x] 3.4 Implement `maybe_start_run()` to spawn `Agent::run_stream` only when `Idle` and queue non-empty.
- [x] 3.5 Implement event forwarding: persist to `events.jsonl`, then broadcast via `EventBroadcaster`.
- [x] 3.6 Implement idle timeout shutdown for controllers without subscribers or active runs.

## 4. V2 REST API routes

- [x] 4.1 Implement `POST /api/v2/sessions` with 201 + Location header and envelope response.
- [x] 4.2 Implement `GET /api/v2/sessions` with cursor pagination by `(updated_at, id)`.
- [x] 4.3 Implement `GET /api/v2/sessions/{id}` and `DELETE /api/v2/sessions/{id}` with user_id checks.
- [x] 4.4 Implement `POST /api/v2/sessions/{id}/prompts` returning 202 and triggering controller.
- [x] 4.5 Implement `POST /api/v2/sessions/{id}/steering` with default priority 255.
- [x] 4.6 Implement `POST /api/v2/sessions/{id}/cancel` canceling current run.
- [x] 4.7 Implement `GET /api/v2/sessions/{id}/events` SSE with `last_seq` replay and `SyncCaughtUp`.
- [x] 4.8 Implement `GET /api/v2/sessions/{id}/messages` with cursor pagination by seq.

## 5. Error response and pagination utilities

- [x] 5.1 Create `ApiError` struct and `IntoResponse` impl returning `{ error: { code, message, details } }`.
- [x] 5.2 Create cursor encoding/decoding utility with base64 URL-safe JSON.
- [x] 5.3 Create envelope response builder (`data`, `meta`, `links`).
- [x] 5.4 Add validation for request bodies using existing or new validation helpers.

## 6. WebSocket and V1 deprecation

- [x] 6.1 Refactor WebSocket handler to only subscribe to `EventBroadcaster`, not spawn runs.
- [x] 6.2 Add `Deprecation: true` header to all V1 route responses.
- [x] 6.3 Ensure V1 SSE/stream routes still work with existing broadcasters created by V2 controller.

## 7. Testing

- [x] 7.1 Add unit tests for `SessionController` state transitions.
- [x] 7.2 Add unit tests for cursor encoding/decoding and pagination edge cases.
- [x] 7.3 Add integration tests for multi-client SSE observation of the same session.
- [x] 7.4 Add integration tests for steering/cancel via HTTP endpoints.
- [x] 7.5 Add integration tests for user_id isolation (cross-user access returns 404).
- [x] 7.6 Add integration tests for SSE `last_seq` replay after disconnect.
- [x] 7.7 Run `cargo clippy --all-targets --all-features --tests --all` and fix warnings.
- [x] 7.8 Run `cargo +nightly fmt --all`.
