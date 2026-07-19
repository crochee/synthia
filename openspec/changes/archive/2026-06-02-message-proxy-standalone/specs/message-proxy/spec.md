## ADDED Requirements

### Requirement: MessageProxy Service SHALL run as a standalone gRPC server

The MessageProxy service SHALL run as an independent process, accessible via gRPC over Unix Domain Socket. It SHALL accept connections from multiple Agent processes and route messages between them.

#### Scenario: MessageProxy starts and listens on configured address
- **WHEN** MessageProxy process starts with `MESSAGE_PROXY_ADDR` set to `/var/run/synthia/message-proxy.sock`
- **THEN** it SHALL create the Unix Domain Socket at that path and accept gRPC connections

#### Scenario: MessageProxy handles multiple concurrent agent connections
- **WHEN** Three Agent processes connect to MessageProxy simultaneously
- **THEN** MessageProxy SHALL handle all connections concurrently without message loss

---

### Requirement: Point-to-Point message delivery SHALL deliver messages to specific agents

When an Agent sends a message with a non-empty `to` field, MessageProxy SHALL route that message directly to the specified Agent. If the target Agent is not connected, the message SHALL be discarded.

#### Scenario: Direct message delivered to connected agent
- **WHEN** Agent A sends a message to Agent B (who is connected)
- **THEN** MessageProxy SHALL deliver the message to Agent B's subscription stream

#### Scenario: Direct message discarded when target agent is not connected
- **WHEN** Agent A sends a message to Agent C (who is not connected)
- **THEN** MessageProxy SHALL discard the message and return SendResult with success=true

---

### Requirement: Broadcast message delivery SHALL deliver messages to multiple agents

When an Agent sends a broadcast request with a list of recipients, MessageProxy SHALL deliver the message to each listed Agent that is currently connected. When recipients list is empty, message SHALL be delivered to all connected Agents.

#### Scenario: Broadcast to specific recipients
- **WHEN** Agent A broadcasts to agents [B, C, D]
- **THEN** MessageProxy SHALL deliver the message to B and D (if connected) and skip C (if not connected)

#### Scenario: Broadcast to all connected agents
- **WHEN** Agent A broadcasts with empty recipients list
- **THEN** MessageProxy SHALL deliver the message to all currently connected agents

---

### Requirement: Agent registration SHALL allow agents to subscribe to messages

An Agent SHALL register with MessageProxy by calling the Register RPC, providing its agent ID. After registration, the Agent SHALL receive messages sent to it via its Subscribe stream.

#### Scenario: Agent registers and receives messages
- **WHEN** Agent B calls Register with agent_id="agent-b"
- **THEN** MessageProxy SHALL add Agent B to its registry and start streaming messages to Agent B via Subscribe

#### Scenario: Agent reconnects after disconnection
- **WHEN** Agent B disconnects and later reconnects with the same agent_id
- **THEN** MessageProxy SHALL accept the reconnection and resume message delivery to Agent B

---

### Requirement: Environment variable configuration SHALL allow flexible connection settings

MessageProxy SHALL read the connection address from `MESSAGE_PROXY_ADDR` environment variable. If not set, it SHALL default to `/var/run/synthia/message-proxy.sock`.

#### Scenario: Default address when env var not set
- **WHEN** MessageProxy starts without `MESSAGE_PROXY_ADDR` set
- **THEN** it SHALL use `/var/run/synthia/message-proxy.sock` as the default address

#### Scenario: Custom address from environment variable
- **WHEN** MessageProxy starts with `MESSAGE_PROXY_ADDR=/custom/path/proxy.sock`
- **THEN** it SHALL use `/custom/path/proxy.sock` as the listening address
