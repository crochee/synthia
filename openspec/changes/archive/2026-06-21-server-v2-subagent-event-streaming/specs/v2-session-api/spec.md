## ADDED Requirements

### Requirement: SessionSummary responses SHALL include the parent_id field

Every `SessionSummary` returned by V2 session endpoints SHALL include `parent_id`, which is `null` for top-level sessions and the parent session id for subagent sessions.

#### Scenario: List sessions includes subagents
- **WHEN** a client sends `GET /api/v2/sessions`
- **THEN** each returned `SessionSummary` contains a `parent_id` field

#### Scenario: Get session detail includes parent_id
- **WHEN** a client sends `GET /api/v2/sessions/{id}`
- **THEN** the returned session detail contains a `parent_id` field

---

### Requirement: SessionFilter SHALL support filtering by parent_id

`SessionManager` internal filtering APIs SHALL accept an optional `parent_id` and return only sessions whose metadata matches.

#### Scenario: Filter sessions by parent
- **WHEN** `SessionManager::list_sessions_for_user` is called with `parent_id = Some(parent_id)`
- **THEN** only sessions with that `parent_id` are returned

---

### Requirement: Session metadata persistence SHALL write parent_id

When a session with `parent_id` is saved, the metadata file SHALL contain the `parent_id` field, and loading it SHALL restore the value.

#### Scenario: Round-trip parent_id
- **WHEN** a child session is saved and then loaded
- **THEN** the loaded metadata has the same `parent_id`
