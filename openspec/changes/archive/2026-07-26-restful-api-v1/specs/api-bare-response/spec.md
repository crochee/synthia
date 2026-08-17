## ADDED Requirements

### Requirement: Bare success response
All Management API success responses SHALL return the resource object directly without envelope. No `status` field, no `data` wrapper.

#### Scenario: Detail endpoint bare return
- **WHEN** `GET /api/v1/skills/debugging` succeeds
- **THEN** the response SHALL be `200` with the SkillDetail object as top-level JSON (e.g., `{ "name": "debugging", "description": "...", "enabled": true }`)

#### Scenario: List endpoint bare return
- **WHEN** `GET /api/v1/skills` succeeds
- **THEN** the response SHALL be `200` with the `List<T>` object as top-level JSON (e.g., `{ "data": [...], "next_cursor": "...", "total": 5 }`)

#### Scenario: Create endpoint bare return
- **WHEN** `POST /api/v1/skills` succeeds
- **THEN** the response SHALL be `201` with the created resource object as top-level JSON

---

### Requirement: Unified DELETE 204
All DELETE endpoints SHALL return HTTP 204 No Content with no response body.

#### Scenario: Delete skill
- **WHEN** `DELETE /api/v1/skills/debugging` succeeds
- **THEN** the response SHALL be HTTP 204 with no body

#### Scenario: Delete MCP server
- **WHEN** `DELETE /api/v1/mcp/servers/filesystem` succeeds
- **THEN** the response SHALL be HTTP 204 with no body

#### Scenario: Delete job
- **WHEN** `DELETE /api/v1/jobs/cleanup` succeeds
- **THEN** the response SHALL be HTTP 204 with no body

---

### Requirement: ApiResponse envelope removal
The `ApiResponse<T>` enum SHALL be deprecated for Management API usage. A2A protocol endpoints MAY continue using their own envelope format internally.

#### Scenario: Management handler returns bare type
- **WHEN** a management handler's return type was `Json<ApiResponse<SkillInfo>>`
- **THEN** it SHALL be changed to `Json<SkillInfo>`

#### Scenario: A2A handler keeps envelope
- **WHEN** an A2A JSON-RPC handler returns a response
- **THEN** it MAY continue using the A2A envelope format per JSON-RPC specification
