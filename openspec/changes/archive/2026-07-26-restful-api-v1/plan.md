# RESTful API v1 Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development
> to implement this plan task-by-task.

**Goal:** Migrate Synthia Management API to RESTful v1 with bare responses, cursor+limit keyset pagination, and unified error format.

**Architecture:** Core types (List\<T\>, PageQuery, UserError IntoResponse) live in synthia-core. Handlers in synthia-server return bare types via axum Json(). Routes migrate from /api/* to /api/v1/*. Frontend (synthia-web) adapts response parsing, error handling, and pagination.

**Tech Stack:** Rust (axum, serde, utoipa, base64), TypeScript (React, @a2a-js/sdk), Playwright (E2E)

---

## Task 1: Core Types in synthia-core

- [ ] **Step 1:** Create `crates/synthia-core/src/api/list.rs` with `List<T>` struct (data, next_cursor, total) and derive Serialize, ToSchema
- [ ] **Step 2:** Create `crates/synthia-core/src/api/page_query.rs` with `PageQuery` (cursor, limit, sort), `TaskPageQuery` (flatten PageQuery + status + context_id), `JobPageQuery` (flatten PageQuery + key + trigger_contains)
- [ ] **Step 3:** Create `crates/synthia-core/src/api/cursor.rs` with encode_cursor, decode_cursor, next_cursor, resolve_page functions using base64 URL_SAFE_NO_PAD
- [ ] **Step 4:** Create `crates/synthia-core/src/api/validation.rs` with validate_resource_name (regex), validate_sort (whitelist), api_key_mask functions
- [ ] **Step 5:** Add `ErrorCode::InvalidCursor` and `ErrorCode::InvalidSortField` variants in synthia-core error types
- [ ] **Step 6:** Implement `IntoResponse` for `UserError` in synthia-core: map ErrorCode → StatusCode, build `{ code, message, result? }` JSON body
- [ ] **Step 7:** Add `crates/synthia-core/src/api/mod.rs` exporting all new types; add `api` module to lib.rs
- [ ] **Step 8:** Run `cargo check -p synthia-core` and `cargo test -p synthia-core` to verify compilation
- [ ] **Step 9:** Commit: "feat(synthia-core): add List<T>, PageQuery, cursor, validation, UserError IntoResponse for v1 API"

## Task 2: Deprecate Old API Infrastructure

- [ ] **Step 1:** Add `#[deprecated(note = "Use bare Json(T) response for v1 API")]` to `json_data()` in `synthia-server/src/api/envelope.rs`
- [ ] **Step 2:** Add deprecation doc comment to `synthia-server/src/api/pagination.rs` (Cursor, PaginatedResponse, Direction)
- [ ] **Step 3:** Refactor `synthia-server/src/api/error.rs`: re-export `UserError` from synthia-core as the primary error type; keep `ApiError` as thin wrapper with deprecation note
- [ ] **Step 4:** Run `cargo check -p synthia-server` to verify no breakage
- [ ] **Step 5:** Commit: "refactor(synthia-server): deprecate envelope/pagination, align error types with synthia-core"

## Task 3: Refactor Skills Handlers

- [ ] **Step 1:** Refactor `list_skills`: return type `Json<List<SkillInfo>>`, build List with cursor encoding, validate sort against whitelist ["name", "created_at"]
- [ ] **Step 2:** Refactor `get_skill`: return `Json<SkillDetail>` bare, validate resource name
- [ ] **Step 3:** Refactor `toggle_skill`: rename route from PATCH to PUT, only modify enabled field, return `Json<SkillEnabled>`
- [ ] **Step 4:** Refactor `delete_skill`: return `(StatusCode::NO_CONTENT, ())` for 204
- [ ] **Step 5:** Refactor `create_skill`: return `(StatusCode::CREATED, Json(skill_detail))`
- [ ] **Step 6:** Run `cargo test -p synthia-server` to verify
- [ ] **Step 7:** Commit: "refactor(synthia-server): skills handlers — bare response, cursor pagination, PUT toggle, 204 delete"

## Task 4: Refactor Tools, Commands, Jobs Handlers

- [ ] **Step 1:** Refactor list/get/delete tools handlers: bare List\<ToolInfo\>, bare ToolDetail, 204 delete
- [ ] **Step 2:** Refactor list/delete commands handlers: bare List\<CommandInfo\>, 204 delete
- [ ] **Step 3:** Refactor list_jobs: use JobPageQuery, return List\<JobInfo\>, validate sort against ["key", "trigger_desc"]
- [ ] **Step 4:** Refactor delete_job: return 204
- [ ] **Step 5:** Add `POST /jobs/:key/pause` and `POST /jobs/:key/resume` handlers (replace toggle pause)
- [ ] **Step 6:** Run `cargo test -p synthia-server`
- [ ] **Step 7:** Commit: "refactor(synthia-server): tools/commands/jobs handlers — bare response, cursor pagination, 204 delete, pause/resume split"

## Task 5: Refactor Tasks, Providers, Settings Handlers

- [ ] **Step 1:** Refactor list_tasks: use TaskPageQuery (status, context_id filters), return List\<TaskSummary\>, validate sort against ["created_at", "updated_at", "status"]
- [ ] **Step 2:** Add `GET /tasks/:id` handler: return TaskDetail with history, artifacts, status as A2A TaskState
- [ ] **Step 3:** Refactor list_providers: return List\<ProviderInfo\> with full fields (base_url, context_window, max_output_tokens, supports_tools, supports_streaming, supports_reasoning, active)
- [ ] **Step 4:** Add `GET /providers/:name` handler: bare ProviderInfo with full fields
- [ ] **Step 5:** Remove POST/PUT/DELETE provider handlers and routes
- [ ] **Step 6:** Refactor get_settings: bare response with api_key masked via api_key_mask()
- [ ] **Step 7:** Refactor put_settings: bare response with api_key masked
- [ ] **Step 8:** Run `cargo test -p synthia-server`
- [ ] **Step 9:** Commit: "refactor(synthia-server): tasks/providers/settings handlers — bare response, TaskPageQuery, provider read-only, API key mask"

## Task 6: Refactor MCP, Memory, Approvals, Models, Health Handlers

- [ ] **Step 1:** Refactor MCP server list: add status (ConnectionStatus) and pid fields to McpServerInfo, return List\<McpServerInfo\>
- [ ] **Step 2:** Refactor MCP server detail: bare McpServerInfo with status/pid
- [ ] **Step 3:** Refactor MCP server delete: cascade unregister tools from that server, return 204
- [ ] **Step 4:** Refactor memory search: use PageQuery for limit/cursor, return List\<MemoryResult\>, sort fixed score DESC
- [ ] **Step 5:** Refactor list_approvals: return List\<ApprovalInfo\>
- [ ] **Step 6:** Refactor resolve_approval: bare `{ resolved: true }`
- [ ] **Step 7:** Refactor list_models: bare response without envelope
- [ ] **Step 8:** Refactor health: bare `{ status, version }`
- [ ] **Step 9:** Run `cargo test -p synthia-server`
- [ ] **Step 10:** Commit: "refactor(synthia-server): mcp/memory/approvals/models/health handlers — bare response, MCP status/pid, cascade delete"

## Task 7: Route Migration

- [ ] **Step 1:** Update router.rs: nest management routes under `/api/v1/` instead of `/api/`
- [ ] **Step 2:** Move MCP JSON-RPC endpoint from `/api/v1/mcp` to `/api/v1/mcp/rpc`
- [ ] **Step 3:** Keep MCP REST at `/api/v1/mcp/servers/*`
- [ ] **Step 4:** Add `/api/v1/tasks/:id` route
- [ ] **Step 5:** Add `/api/v1/jobs/:key/pause` and `/api/v1/jobs/:key/resume` routes
- [ ] **Step 6:** Remove provider write routes
- [ ] **Step 7:** Update authentication whitelist for all new route paths
- [ ] **Step 8:** Add 301 redirect from `/api/*` to `/api/v1/*` for transition period
- [ ] **Step 9:** Run `cargo test -p synthia-server`
- [ ] **Step 10:** Commit: "refactor(synthia-server): routes — /api/v1/* prefix, MCP RPC separation, task detail, jobs pause/resume, provider read-only"

## Task 8: Registry Trait Enhancement

- [ ] **Step 1:** Add `async fn list_paginated(&self, cursor: Option<String>, limit: u64, sort: Option<String>, filter: Option<Self::Filter>) -> Result<List<E>, Error>` to Registry trait
- [ ] **Step 2:** Implement default `list_paginated` for in-memory registries: call list(), apply sort, slice by cursor+limit, build List<T>
- [ ] **Step 3:** Run `cargo check --workspace` and `cargo test --workspace` (per-module)
- [ ] **Step 4:** Commit: "feat(synthia-core): add list_paginated to Registry trait with in-memory default impl"

## Task 9: Frontend Adaptation

- [ ] **Step 1:** Update API client base URL constant from `/api/` to `/api/v1/`
- [ ] **Step 2:** Update all API response parsers: remove `response.data` envelope access, use response directly
- [ ] **Step 3:** Update error handling: replace `response.status === "err"` with HTTP status code checks (response.ok, response.status)
- [ ] **Step 4:** Update list data access: `response.items` → `response.data` everywhere
- [ ] **Step 5:** Update DELETE handlers: expect 204 (no body), handle no-content response
- [ ] **Step 6:** Update POST handlers: expect 201 status for create operations
- [ ] **Step 7:** Add cursor-based pagination component: store next_cursor, pass as ?cursor= on next page
- [ ] **Step 8:** Update Skills toggle: change from PATCH to PUT request
- [ ] **Step 9:** Update Settings: implement read-modify-write pattern (GET → local modify → PUT full object)
- [ ] **Step 10:** Add Task detail page: fetch `GET /api/v1/tasks/:id`, display history and artifacts
- [ ] **Step 11:** Update MCP page: display status and pid fields in server list
- [ ] **Step 12:** Remove provider create/edit/delete UI elements (read-only)
- [ ] **Step 13:** Run `tsc --noEmit` and Playwright E2E tests
- [ ] **Step 14:** Commit: "feat(synthia-web): adapt to v1 API — bare response, cursor pagination, 204 delete, provider read-only"

## Task 10: Integration Tests

- [ ] **Step 1:** Update all existing integration test assertions: remove ApiResponse envelope checks, assert bare response fields directly
- [ ] **Step 2:** Update DELETE test assertions: expect 204 status, no body
- [ ] **Step 3:** Add test: list pagination — first page, next_cursor, subsequent page, last page (null cursor)
- [ ] **Step 4:** Add test: limit boundaries — limit=0 (400), limit>100 (truncated), default (20)
- [ ] **Step 5:** Add test: cursor encode/decode — valid cursor, invalid base64 (400), deleted resource ID (empty list)
- [ ] **Step 6:** Add test: sort whitelist — valid field (200), invalid field (400), default sort
- [ ] **Step 7:** Add test: resource name validation — valid name, path traversal (400), empty (400)
- [ ] **Step 8:** Add test: API key masking — long key (masked), null key, short key (***)
- [ ] **Step 9:** Add test: Task detail endpoint — found (200), not found (404), with history/artifacts
- [ ] **Step 10:** Add test: TaskPageQuery filter — status filter, context_id filter, invalid status (400)
- [ ] **Step 11:** Add test: Jobs pause/resume — separate endpoints, correct status responses
- [ ] **Step 12:** Add test: MCP server cascade delete — tools unregistered after server delete
- [ ] **Step 13:** Add test: Provider read-only — GET works, POST/PUT/DELETE return 404
- [ ] **Step 14:** Add test: Error response format — all error types return { code, message, result? }
- [ ] **Step 15:** Run `cargo test -p synthia-server` (per test module)
- [ ] **Step 16:** Commit: "test(synthia-server): v1 API integration tests — pagination, errors, validation, new endpoints"

## Task 11: Format, Lint, Final Verification

- [x] **Step 1:** Run `cargo +nightly fmt --all`
- [x] **Step 2:** Run `cargo clippy --all-targets --all-features --tests --all` and fix all warnings
- [x] **Step 3:** Run `cargo test -p synthia-core` and `cargo test -p synthia-server` (per module)
- [x] **Step 4:** Run `tsc --noEmit` in synthia-web
- [x] **Step 5:** Run Playwright E2E tests
- [ ] **Step 6:** Commit: "chore: fmt + clippy + final test verification for v1 API migration"
