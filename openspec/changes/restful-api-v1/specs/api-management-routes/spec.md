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
MCP JSON-RPC endpoint SHALL be at `POST /api