## ADDED Requirements

### Requirement: MCP heartbeat SHALL send ping every 30 seconds when connection is idle
The MCP connection SHALL implement a heartbeat mechanism that sends a ping request when no activity is detected for a configurable interval (default 30 seconds).

#### Scenario: Idle connection receives heartbeat
- **WHEN** MCP connection is in Connected state with no outgoing requests for 30 seconds
- **THEN** A ping request SHALL be sent and a timeout timer SHALL start waiting for pong response

### Requirement: MCP heartbeat SHALL disconnect on pong timeout
If no pong response is received within 10 seconds of sending ping, the connection SHALL be considered dead and transition to Error state.

#### Scenario: Server does not respond to ping
- **WHEN** Ping is sent but no pong received within 10 seconds
- **THEN** Connection SHALL transition to Error state and SHALL attempt reconnection

### Requirement: MCP heartbeat SHALL reset on any successful request/response
Any successful JSON-RPC request or response SHALL reset the idle timer, ensuring active connections are not affected by heartbeat.

#### Scenario: Active connection resets heartbeat timer
- **WHEN** A tool call or other JSON-RPC request completes successfully
- **THEN** The idle timer SHALL be reset and no ping SHALL be sent until next idle period

---

## MODIFIED Requirements

### Requirement: Connection state machine SHALL include Idle state with proper transitions
The ConnectionState enum SHALL include an Idle state. Transitions: Discovered → Connecting → Connected → Idle → Connected (on reconnect) or Error.

#### Scenario: Connection enters idle after successful handshake
- **WHEN** MCP connection completes initialization and enters Connected state
- **THEN** Idle timer SHALL start counting from the last activity timestamp

---

## REMOVED Requirements

### Requirement: (None - no requirements being removed)

---

## RENAMED Requirements

### Requirement: (None - no requirements being renamed)