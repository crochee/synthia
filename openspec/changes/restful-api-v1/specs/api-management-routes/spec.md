## MODIFIED Requirements

### Requirement: Route version prefix
All Management API routes SHALL use `/api/v1/` prefix instead of `/api/`.

#### Scenario: Skills list route
- **WHEN** listing skills
- **THEN** the route SHALL be `GET /api/v1/skills`

#### Scenario: Settings route
- **WHEN** getting settings
- **THEN** the route SHALL be `GET /api/v1/settings`

---

### Requirement: Providers read-only
Providers SHALL be a read-only resource with only `GET /api/v1/providers` and `GET /api/v1/providers/:name` endpoints. POST, PUT, and DELETE endpoints SHALL be removed.

#### Scenario: List providers
- **WHEN** `GET /api/v1/providers`
- **THEN** the system SHALL return `List<ProviderInfo>` with full fields including base_url, context_window, max_output_tokens, supports_tools, supports_streaming, supports_reasoning, active

#### Scenario: Get provider detail
- **WHEN** `GET /api/v1/providers/openai`
- **THEN** the system SHALL return ProviderInfo with all fields as bare response

#### Scenario: Provider write operations removed
- **WHEN** `POST /api/v1/providers` or `PUT /api/v1/providers/:name` or `DELETE /api/v1/providers/:name`
- **THEN** the system SHALL return HTTP 404 (route not registered)

---

### Requirement: Skills PUT semantics
`PUT /api/v1/skills/:name` SHALL only modify the `enabled` field. Other fields (name, description, path) are derived from the filesystem and SHALL be ignored in the request body.

#### Scenario: Toggle skill enabled
- **WHEN** `PUT /api/v1/skills/debugging` with body `{ "enabled": false }`
- **THEN** the system SHALL update the skill's enabled flag and return `200 { "name": "debugging", "enabled": false }`

#### Scenario: PUT with extra fields ignored
- **WHEN** `PUT /api/v1/skills/debugging` with body `{ "enabled": true, "description": "hacked" }`
- **THEN** the system SHALL only update `enabled` and ignore `description`; the response SHALL reflect the filesystem-derived description

---

### Requirement: Settings PUT full replacement
`PUT /api/v1/settings` SHALL perform full replacement. Clients MUST read-modify-write: GET first, modify desired fields, then PUT the complete object. Missing fields SHALL be set to their default/null values.

#### Scenario: Full replacement
- **WHEN** `PUT /api/v1/settings` with `{ "provider": "groq", "model": "llama-3", "api_key": "", "skills": {} }`
- **THEN** all settings SHALL be replaced with the provided values

#### Scenario: Missing field cleared
- **WHEN** `PUT /api/v1/settings` with `{ "model": "llama-3" }` (no provider field)
- **THEN** `provider` SHALL be set to null/default

---

### Requirement: MCP JSON-RPC path separation
MCP JSON-RPC endpoint SHALL be at `POST /api/v1/mcp/rpc`. MCP REST management endpoints SHALL remain at `/api/v1/mcp/servers/*`.

#### Scenario: JSON-RPC call
- **WHEN** a JSON-RPC request is sent to `POST /api/v1/mcp/rpc`
- **THEN** the system SHALL process it as a JSON-RPC request and return `{ "jsonrpc": "2.0", "id": N, "result": ... }`

#### Scenario: REST list servers
- **WHEN** `GET /api/v1/mcp/servers`
- **THEN** the system SHALL return `List<McpServerInfo>`

---

### Requirement: Tasks detail endpoint
The system SHALL provide `GET /api/v1/tasks/:id` returning full TaskDetail including history and artifacts, using A2A TaskState for status values.

#### Scenario: Task detail
- **WHEN** `GET /api/v1/tasks/task_abc`
- **THEN** the system SHALL return `200` with TaskDetail including id, status (A2A TaskState), context_id, created_at, updated_at, history, artifacts

#### Scenario: Task not found
- **WHEN** `GET /api/v1/tasks/nonexistent`
- **THEN** the system SHALL return `404 { "code": "not_found", "message": "Task 'nonexistent' not found" }`

---

### Requirement: Jobs pause and resume separation
Jobs SHALL have separate `POST /api/v1/jobs/:key/pause` and `POST /api/v1/jobs/:key/resume` endpoints instead of a single toggle.

#### Scenario: Pause job
- **WHEN** `POST /api/v1/jobs/cleanup/pause`
- **THEN** the system SHALL pause the job and return `200 { "key": "cleanup", "status": "paused" }`

#### Scenario: Resume job
- **WHEN** `POST /api/v1/jobs/cleanup/resume`
- **THEN** the system SHALL resume the job and return `200 { "key": "cleanup", "status": "resumed" }`

---

### Requirement: MCP Server connection status
McpServerInfo SHALL include `status` (ConnectionStatus: starting/connected/disconnected/error) and `pid` (Option<u32>) fields.

#### Scenario: Connected MCP server
- **WHEN** `GET /api/v1/mcp/servers/filesystem` and the server is connected
- **THEN** the response SHALL include `{ "status": "connected", "pid": 12345 }`

#### Scenario: Disconnected MCP server
- **WHEN** `GET /api/v1/mcp/servers/filesystem` and the server is disconnected
- **THEN** the response SHALL include `{ "status": "disconnected", "pid": null }`

---

### Requirement: API Key masking
Settings responses SHALL mask api_key values, keeping the first 4 and last 3 characters with `***` in between. Empty or null api_key SHALL remain as-is.

#### Scenario: Masked API key in response
- **WHEN** `GET /api/v1/settings` and api_key is "sk-proj-abc123xyz"
- **THEN** the response SHALL include `"api_key": "sk-p***xyz"`

#### Scenario: Null API key
- **WHEN** `GET /api/v1/settings` and api_key is null
- **THEN** the response SHALL include `"api_key": null`

#### Scenario: Short API key
- **WHEN** `GET /api/v1/settings` and api_key is "abc" (less than 7 chars)
- **THEN** the response SHALL include `"api_key": "***"`

---

### Requirement: MCP Server cascade delete
Deleting an MCP server SHALL also unregister all tools that were registered from that server.

#### Scenario: Delete server with registered tools
- **WHEN** `DELETE /api/v1/mcp/servers/filesystem` and the server has registered tools ["read_file", "write_file"]
- **THEN** the system SHALL unregister those tools and return HTTP 204
