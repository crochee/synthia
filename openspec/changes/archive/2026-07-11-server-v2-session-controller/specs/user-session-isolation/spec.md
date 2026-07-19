## ADDED Requirements

### Requirement: Session creation binds user_id
The server SHALL create sessions with the authenticated user's id derived from the Bearer token.

#### Scenario: Authenticated user creates session
- **WHEN** a client with a valid Bearer token sends `POST /api/v2/sessions`
- **THEN** the server MUST call `SessionManager::create_with_user(user_id)` and persist the session under `{sessions_root}/{user_id}/{session_id}/`

### Requirement: List filtered by user_id
The server SHALL filter session list results to the authenticated user.

#### Scenario: User A lists sessions
- **WHEN** user A sends `GET /api/v2/sessions`
- **THEN** the response MUST contain only sessions whose `owner_user_id` equals user A's id

### Requirement: Read and delete require ownership
The server SHALL reject read/delete operations on sessions not owned by the authenticated user.

#### Scenario: User A accesses user B's session
- **WHEN** user A sends `GET /api/v2/sessions/{user_b_session_id}` or `DELETE /api/v2/sessions/{user_b_session_id}`
- **THEN** the server MUST respond `404 Not Found` and MUST NOT return session data

### Requirement: Session disk path isolation
The server SHALL store each user's sessions in a separate directory namespace.

#### Scenario: Session directory layout
- **WHEN** a session is created for user `u-abc123` with id `sess-xyz`
- **THEN** the server MUST store files at `{sessions_root}/u-abc123/sess-xyz/` and MUST NOT place user B's files under the same path

### Requirement: Memory index isolation
The in-memory session index SHALL be keyed by `(user_id, session_id)`.

#### Scenario: Same session id across users
- **WHEN** user A and user B both have a session with id `sess-123`
- **THEN** the server MUST treat them as distinct sessions and MUST NOT leak events or state between them
