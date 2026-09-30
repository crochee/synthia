## ADDED Requirements

### Requirement: GET /api/v2/sessions/{id}/subagents SHALL return child sessions for the given parent

The endpoint SHALL list all sessions whose `parent_id` equals `{id}` and that belong to the authenticated user.

#### Scenario: Parent has active subagents
- **WHEN** a client sends `GET /api/v2/sessions/{parent_id}/subagents`
- **THEN** the response contains a paginated list of child sessions with `parent_id == {parent_id}`

---

### Requirement: The subagents endpoint SHALL enforce user isolation

The endpoint SHALL return `404 Not Found` if the parent session does not belong to the caller, and SHALL NOT expose child sessions of another user.

#### Scenario: Cross-user access
- **WHEN** user `u2` sends `GET /api/v2/sessions/{u1_parent_id}/subagents`
- **THEN** the response is `404 Not Found`

---

### Requirement: The subagents endpoint SHALL support cursor pagination

The endpoint SHALL accept an optional `cursor` query parameter and return `has_next` plus `next_cursor` in the response metadata.

#### Scenario: Many subagents
- **WHEN** a parent has more children than the page size
- **THEN** the first response has `has_next: true` and a non-null `next_cursor`
- **AND** the next request with that cursor returns the following children
