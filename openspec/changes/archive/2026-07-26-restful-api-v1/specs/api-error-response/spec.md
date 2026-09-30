## ADDED Requirements

### Requirement: UserError IntoResponse
The system SHALL implement `IntoResponse` for `UserError` that returns HTTP Status + JSON body `{ "code": string, "message": string }` with optional `"result"` field.

#### Scenario: Simple error without result
- **WHEN** a resource is not found
- **THEN** the response SHALL be HTTP 404 with body `{ "code": "not_found", "message": "<descriptive message>" }` and no `result` field

#### Scenario: Error with result details
- **WHEN** request validation fails with field-level details
- **THEN** the response SHALL be HTTP 422 with body `{ "code": "validation_error", "message": "...", "result": { "field": "...", "issue": "..." } }`

#### Scenario: Rate limit error with result
- **WHEN** rate limit is triggered
- **THEN** the response SHALL be HTTP 429 with body `{ "code": "rate_limited", "message": "Too many requests", "result": { "retry_after_seconds": N, "limit": N, "remaining": 0 } }`

---

### Requirement: ErrorCode to HTTP Status mapping
The system SHALL map ErrorCode variants to HTTP status codes: bad_request→400, unauthorized→401, forbidden→403, not_found→404, conflict→409, already_exists→409, validation_error→422, rate_limited→429, internal_server_error→500, service_unavailable→503. Unmapped variants SHALL default to 500.

#### Scenario: NotFound maps to 404
- **WHEN** UserError with code `not_found` is returned
- **THEN** HTTP response status SHALL be 404

#### Scenario: Unmapped error defaults to 500
- **WHEN** UserError with an unmapped ErrorCode is returned
- **THEN** HTTP response status SHALL be 500

---

### Requirement: New ErrorCode variants
The system SHALL add `InvalidCursor` (→400) and `InvalidSortField` (→400) to the ErrorCode enum.

#### Scenario: InvalidCursor error
- **WHEN** a cursor parameter cannot be decoded
- **THEN** the response SHALL be HTTP 400 with code `invalid_cursor`

#### Scenario: InvalidSortField error
- **WHEN** a sort parameter contains a field not in the whitelist
- **THEN** the response SHALL be HTTP 400 with code `invalid_sort_field`

---

### Requirement: Resource name validation
The system SHALL validate all resource name path parameters against regex `^[a-zA-Z0-9_-]{1,255}$`. Invalid names SHALL return HTTP 400 with code `bad_request`.

#### Scenario: Valid resource name
- **WHEN** `GET /api/v1/skills/debugging` with name "debugging"
- **THEN** the system SHALL process the request normally

#### Scenario: Resource name with path traversal
- **WHEN** `GET /api/v1/skills/../etc/passwd`
- **THEN** the system SHALL return HTTP 400 with code `bad_request`

#### Scenario: Empty resource name
- **WHEN** a path parameter is empty
- **THEN** the system SHALL return HTTP 400 with code `bad_request`
