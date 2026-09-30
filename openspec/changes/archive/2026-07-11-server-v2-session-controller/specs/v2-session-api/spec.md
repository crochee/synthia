## ADDED Requirements

### Requirement: Session creation endpoint
The server SHALL expose `POST /api/v2/sessions` to create a session bound to the authenticated user.

#### Scenario: Authenticated user creates a session
- **WHEN** a client sends `POST /api/v2/sessions` with a valid Bearer token and optional `model`, `max_iterations`, `title`
- **THEN** the server MUST respond `201 Created` with a `Location` header pointing to `/api/v2/sessions/{id}` and a body containing the new session `id`, `user_id`, `state`, `model`, `title`, `created_at`, `updated_at`

### Requirement: Session list endpoint
The server SHALL expose `GET /api/v2/sessions` to list sessions belonging to the authenticated user.

#### Scenario: User lists own sessions
- **WHEN** a client sends `GET /api/v2/sessions` with a valid Bearer token
- **THEN** the server MUST return only sessions whose `user_id` matches the token-derived user_id

### Requirement: Session detail endpoint
The server SHALL expose `GET /api/v2/sessions/{id}` to return session details.

#### Scenario: Owner reads session details
- **WHEN** a client sends `GET /api/v2/sessions/{id}` with a valid Bearer token for a session it owns
- **THEN** the server MUST return the session details including `id`, `state`, `model`, `iteration`, `cumulative_tokens`, `created_at`, `updated_at`

#### Scenario: Non-owner reads session details
- **WHEN** a client sends `GET /api/v2/sessions/{id}` for a session belonging to another user
- **THEN** the server MUST respond `404 Not Found` and MUST NOT leak the session's existence

### Requirement: Session delete endpoint
The server SHALL expose `DELETE /api/v2/sessions/{id}` to delete a session.

#### Scenario: Owner deletes session
- **WHEN** a client sends `DELETE /api/v2/sessions/{id}` for a session it owns
- **THEN** the server MUST respond `204 No Content` and remove the session from disk and memory index

### Requirement: Prompt endpoint
The server SHALL expose `POST /api/v2/sessions/{id}/prompts` to admit a user prompt into the session input queue.

#### Scenario: User sends a prompt
- **WHEN** a client sends `POST /api/v2/sessions/{id}/prompts` with `{ "content": "...", "priority": 128 }` for a session it owns
- **THEN** the server MUST append the prompt to `session_input.jsonl`, return `202 Accepted` with `{ "seq", "admitted", "state" }`, and trigger the session controller to start or continue a run

### Requirement: Steering endpoint
The server SHALL expose `POST /api/v2/sessions/{id}/steering` to send a high-priority steering message.

#### Scenario: User sends steering
- **WHEN** a client sends `POST /api/v2/sessions/{id}/steering` with `{ "content": "..." }` for a session it owns
- **THEN** the server MUST append the message to `session_input.jsonl` with default priority `255`, return `202 Accepted`, and emit a `SteeringReceived` event

### Requirement: Cancel endpoint
The server SHALL expose `POST /api/v2/sessions/{id}/cancel` to cancel the current run.

#### Scenario: User cancels running session
- **WHEN** a client sends `POST /api/v2/sessions/{id}/cancel` for a session in `Running` state
- **THEN** the server MUST cancel the current run's `CancellationToken`, return `200 OK` with `{ "cancelled": true, "state": "Cancelled" }`, and emit `SessionEnded { reason: Cancelled }`

### Requirement: Events SSE endpoint
The server SHALL expose `GET /api/v2/sessions/{id}/events?last_seq={N}` as an SSE stream.

#### Scenario: Client subscribes to events
- **WHEN** a client sends `GET /api/v2/sessions/{id}/events` with a valid Bearer token
- **THEN** the server MUST stream all subsequent `AgentEvent` variants as SSE events with the existing `{ "type": "...", "data": {...} }` JSON shape

### Requirement: Messages endpoint
The server SHALL expose `GET /api/v2/sessions/{id}/messages` to retrieve conversation messages.

#### Scenario: User reads messages
- **WHEN** a client sends `GET /api/v2/sessions/{id}/messages` for a session it owns
- **THEN** the server MUST return messages from `messages.jsonl` in an envelope with `data`, `meta`, and `links`
