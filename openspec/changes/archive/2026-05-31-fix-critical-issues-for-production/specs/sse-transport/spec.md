## ADDED Requirements

### Requirement: SSE transport SHALL use real HTTP SSE streaming
The SseTransport SHALL make actual HTTP GET requests to the SSE endpoint and parse Server-Sent Events from the response body using standard SSE format (data: prefix, double newline for event end).

#### Scenario: SSE endpoint returns valid events
- **WHEN** HTTP GET to SSE URL returns 200 with text/event-stream content type
- **THEN** Each data: line SHALL be parsed and forwarded to the transport read half

### Requirement: SSE transport SHALL support POST for outgoing messages
The transport SHALL send outgoing JSON-RPC messages via HTTP POST to the configured post_url with Content-Type: application/json.

#### Scenario: Sending JSON-RPC request
- **WHEN** JSON-RPC request is written to stdin_writer
- **THEN** It SHALL be POSTed to the post_url with proper headers

### Requirement: SSE transport SHALL handle connection errors gracefully
On SSE connection failure, the transport SHALL emit Close event and allow reconnection attempts.

#### Scenario: SSE endpoint is unreachable
- **WHEN** GET request to SSE URL fails with network error
- **THEN** SseMessage::Close SHALL be sent and is_connected() SHALL return false

---

## MODIFIED Requirements

### Requirement: SseTransport SHALL NOT use tokio io DuplexStream for simulation
The implementation SHALL NOT use tokio::io::DuplexStream to simulate bidirectional communication. Real network streams SHALL be used instead.

#### Scenario: Transport initialization
- **WHEN** SseTransport::new() is called
- **THEN** Real HTTP client connections SHALL be established, not in-memory duplex streams

---

## REMOVED Requirements

### Requirement: (None - no requirements being removed)

---

## RENAMED Requirements

### Requirement: (None - no requirements being renamed)