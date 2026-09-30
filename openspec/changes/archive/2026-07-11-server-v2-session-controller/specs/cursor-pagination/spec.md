## ADDED Requirements

### Requirement: Sessions list cursor pagination
The `GET /api/v2/sessions` endpoint SHALL support cursor-based pagination sorted by `updated_at` descending.

#### Scenario: First page request
- **WHEN** a client sends `GET /api/v2/sessions?limit=20` without a cursor
- **THEN** the server MUST return up to 20 most recently updated sessions and include `meta.has_next` plus `links.next` if more sessions exist

#### Scenario: Subsequent page request
- **WHEN** a client sends `GET /api/v2/sessions?cursor={cursor}&limit=20`
- **THEN** the server MUST return sessions updated before the cursor's `updated_at` (or same timestamp with id greater than cursor's id), and MUST include `meta.next_cursor` when additional pages exist

### Requirement: Messages cursor pagination
The `GET /api/v2/sessions/{id}/messages` endpoint SHALL support cursor-based pagination by message sequence.

#### Scenario: Backward pagination
- **WHEN** a client sends `GET /api/v2/sessions/{id}/messages?limit=20&direction=backward`
- **THEN** the server MUST return up to 20 newest messages in reverse chronological order and provide a `next_cursor` for older messages

#### Scenario: Forward pagination
- **WHEN** a client sends `GET /api/v2/sessions/{id}/messages?cursor={cursor}&limit=20&direction=forward`
- **THEN** the server MUST return messages with sequence greater than the cursor's seq in chronological order

### Requirement: Opaque cursor encoding
Cursors SHALL be opaque base64-encoded JSON strings to prevent clients from constructing or interpreting cursor internals.

#### Scenario: Cursor decoding
- **WHEN** the server receives a cursor parameter
- **THEN** it MUST base64-decode the cursor and deserialize it as JSON; if decoding fails, the server MUST respond `400 Bad Request` with error code `invalid_cursor`

### Requirement: Cursor stability
The cursor for a given item SHALL remain valid as new items are appended.

#### Scenario: New sessions created after cursor
- **WHEN** a client requests the next page using a cursor obtained before new sessions were created
- **THEN** the server MUST NOT include the newly created sessions in that page and MUST NOT skip or duplicate items from the original page
