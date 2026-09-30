## ADDED Requirements

### Requirement: Events JSONL append-only persistence
The server SHALL append every `AgentEvent` emitted by a session run to an append-only `events.jsonl` file in the session directory.

#### Scenario: Event emitted during run
- **WHEN** `Agent::run_stream` emits a `ToolCallStarted` event
- **THEN** the server MUST append a line to `events.jsonl` containing `seq`, `aggregate`, `type`, `ts`, `source`, and `payload`

### Requirement: Event envelope format
Each persisted event SHALL use a stable envelope format compatible with replay and future sync.

#### Scenario: Persisted event structure
- **WHEN** any event is written to `events.jsonl`
- **THEN** the line MUST be valid JSON containing `seq` (monotonic integer), `aggregate` (session id), `type` (AgentEvent variant name), `ts` (ISO-8601 UTC), `source` (`agent` | `user` | `system`), and `payload` (variant-specific data)

### Requirement: SSE replay from last_seq
The SSE endpoint SHALL replay events from `events.jsonl` starting after `last_seq` before streaming live events.

#### Scenario: Client reconnects with last_seq
- **WHEN** a client connects to `GET /api/v2/sessions/{id}/events?last_seq=42`
- **THEN** the server MUST first push all events from `events.jsonl` with `seq > 42` in order, then continue with live events, and finally emit `SyncCaughtUp` with the current seq

### Requirement: Broadcast after persistence
The server SHALL persist an event to `events.jsonl` before broadcasting it to SSE subscribers.

#### Scenario: Event ordering
- **WHEN** a new event is produced
- **THEN** the server MUST write it to `events.jsonl` and increment `seq` BEFORE sending it to the broadcaster

### Requirement: Backward compatibility
The introduction of `events.jsonl` SHALL NOT break existing session recovery from `messages.jsonl`.

#### Scenario: Old session without events.jsonl
- **WHEN** a session directory contains `messages.jsonl` but no `events.jsonl`
- **THEN** the server MUST treat it as an older session, skip replay, and begin appending events from `seq=1`
