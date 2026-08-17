## 1. Core Types (synthia-core)

- [x] 1.1 Add `List<T>` generic type with `data: Vec<T>`, `next_cursor: Option<String>`, `total: Option<u64>` to synthia-core public types
- [x] 1.2 Add `PageQuery` struct with `cursor: Option<String>`, `limit: Option<u64>`, `sort: Option<String>` and constants DEFAULT_LIMIT=20, MAX_LIMIT=100
- [x] 1.3 Add `TaskPageQuery` extending PageQuery with `status: Option<String>`, `context_id: Option<String>`
- [x] 1.4 Add `JobPageQuery` extending PageQuery with `key: Option<String>`, `trigger_contains: Option<String>`
- [x] 1.5 Add cursor encode/decode functions: `encode_cursor(id: &str) -> String`, `decode_cursor(cursor: &str) -> Result<String, UserError>`
- [x] 1.6 Add `next_cursor(last_item_id: &str, has_more: bool) -> Option<String>` helper
- [x] 1.7 Add `ErrorCode::InvalidCursor` and `ErrorCode::InvalidSortField` variants
- [x] 1.8 Implement `IntoResponse` for `UserError` with HTTP Status + `{ code, message, result? }` format
- [x] 1.9 Add resource name validation function: `validate_resource_name(name: &str) -> Result<(), UserError>` with regex `^[a-zA-Z0-9_-]{1,255}$`
- [x] 1.10 Add `api_key_mask(key: &str) -> String` function for API Key masking (keep first 4 + last 3, middle `***`)
- [x] 1.11 Add sort whitelist validation: `validate_sort(sort: &str, whitelist: &[&str]) -> Result<(), UserError>`

## 2. Deprecated Module Cleanup (synthia-server)

- [x] 2.1 Mark `src/api/envelope.rs` as deprecated, add `#![deprecated]` or doc comment
- [x] 2.2 Mark `src/api/pagination.rs` as deprecated
- [x] 2.3 Refactor `src/api/error.rs` to use `UserError` from synthia-core instead of local `ApiError`
- [x] 2.4 Remove `ApiResponse<T>` usage from all management handlers (keep for A2A if needed) — completed in Tasks 3-6

## 3. Handler Refactoring (synthia-server)

- [x] 3.1 Refactor `list_skills` handler: return `Json<List<SkillInfo>>` instead of `ApiResponse<SkillListResponse>`
- [x] 3.2 Refactor `get_skill` handler: return `Json<SkillDetail>` bare response
- [x] 3.3 Refactor `toggle_skill` handler: change from PATCH to PUT, only modify enabled field
- [x] 3.4 Refactor `delete_skill` handler: return 204 No Content
- [x] 3.5 Refactor `list_tools` handler: return `Json<List<ToolInfo>>`
- [x] 3.6 Refactor `get_tool` handler: return `Json<ToolDetail>` bare response
- [x] 3.7 Refactor `delete_tool` handler: return 204 No Content
- [x] 3.8 Refactor `list_commands` handler: return `Json<List<CommandInfo>>`
- [x] 3.9 Refactor `delete_command` handler: return 204 No Content
- [x] 3.10 Refactor `list_jobs` handler: return `Json<List<JobInfo>>` with JobPageQuery
- [x] 3.11 Refactor `delete_job` handler: return 204 No Content
- [x] 3.12 Add `POST /jobs/:key/pause` and `POST /jobs/:key/resume` separate endpoints (replace toggle)
- [x] 3.13 Refactor `list_approvals` handler: return `Json<List<ApprovalInfo>>`
- [x] 3.14 Refactor `resolve_approval` handler: bare response with `{ resolved: true }`
- [x] 3.15 Refactor `list_tasks` handler: return `Json<List<TaskSummary>>` with TaskPageQuery filter
- [x] 3.16 Add `GET /tasks/:id` handler: return `Json<TaskDetail>` with history and artifacts
- [x] 3.17 Refactor `get_settings` handler: bare response with api_key masked
- [x] 3.18 Refactor `put_settings` handler: bare response with api_key masked
- [x] 3.19 Refactor `list_providers` handler: return `Json<List<ProviderInfo>>` with full fields (base_url, context_window, etc.)
- [x] 3.20 Add `GET /providers/:name` handler: bare ProviderInfo
- [x] 3.21 Remove POST/PUT/DELETE provider handlers
- [x] 3.22 Refactor MCP server list: return `Json<List<McpServerInfo>>` with status and pid fields
- [x] 3.23 Refactor MCP server detail: return `Json<McpServerInfo>` bare with status and pid
- [x] 3.24 Refactor MCP server delete: return 204 + cascade unregister tools
- [x] 3.25 Refactor memory search: use PageQuery for limit/cursor, return `Json<List<MemoryResult>>`
- [x] 3.26 Refactor `list_models` handler: bare response (no envelope)
- [x] 3.27 Refactor health endpoint: bare response `{ status, version }`

## 4. Route Migration

- [x] 4.1 Migrate all management routes from `/api/*` to `/api/v1/*`
- [x] 4.2 Move MCP JSON-RPC endpoint to `/api/v1/mcp/rpc`
- [x] 4.3 Keep MCP REST endpoints at `/api/v1/mcp/servers/*`
- [x] 4.4 Remove provider write routes (POST/PUT/DELETE)
- [x] 4.5 Add `/api/v1/tasks/:id` route
- [x] 4.6 Add `/api/v1/jobs/:key/pause` and `/api/v1/jobs/:key/resume` routes
- [x] 4.7 Update authentication whitelist for new route paths
- [x] 4.8 Add redirect/deprecation for old `/api/*` routes (301 → `/api/v1/*`)

## 5. Registry Trait (long-term preparation)

- [x] 5.1 Add `list_paginated(cursor, limit, sort, filter)` method to Registry trait (default impl calls list + in-memory slice)
- [x] 5.2 Implement cursor-based slicing in default `list_paginated` for in-memory registries

## 6. Frontend Adaptation (synthia-web)

- [x] 6.1 Update API client base URL from `/api/` to `/api/v1/`
- [x] 6.2 Update response parsing: remove `response.data` envelope access, use response directly
- [x] 6.3 Update error handling: check HTTP status codes instead of `response.status === "err"`
- [x] 6.4 Update list parsing: `response.items` → `response.data`, add cursor pagination support
- [x] 6.5 Update DELETE handlers: expect 204 (no body) instead of 200
- [x] 6.6 Update POST handlers: expect 201 for create operations
- [x] 6.7 Add cursor-based pagination UI component (next_cursor → load more / next page)
- [x] 6.8 Update Skills toggle: PATCH → PUT
- [x] 6.9 Update Settings: ensure read-modify-write pattern (GET → modify → PUT)
- [x] 6.10 Add Task detail page using `GET /api/v1/tasks/:id`
- [x] 6.11 Update MCP page: use new status/pid fields in server list
- [x] 6.12 Remove provider create/edit/delete UI (read-only)

## 7. Integration Tests

- [x] 7.1 Update all existing integration test assertions for bare response format
- [x] 7.2 Update all existing integration test assertions for 204 DELETE responses
- [x] 7.3 Add tests for List<T> pagination (cursor encode/decode, limit boundaries, empty results)
- [x] 7.4 Add tests for error response format (HTTP Status + { code, message, result? })
- [x] 7.5 Add tests for sort whitelist validation (valid/invalid fields)
- [x] 7.6 Add tests for resource name validation (valid/traversal/empty)
- [x] 7.7 Add tests for API Key masking
- [x] 7.8 Add tests for Task detail endpoint and TaskPageQuery filter
- [x] 7.9 Add tests for Jobs pause/resume separation
- [x] 7.10 Add tests for MCP server cascade delete
- [x] 7.11 Add tests for Provider read-only enforcement
