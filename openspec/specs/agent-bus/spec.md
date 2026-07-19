# agent-bus Specification

## Purpose
Defines the `AgentBus` trait for inter-agent communication: register, send, broadcast, and subscribe.
Implementations (`MemoryAgentBus`, `FileAgentBus`, `MessageProxyAgentBus`) are interchangeable behind the trait,
giving the runtime a single seam for substituting in-process vs cross-process message transport without
touching agent call sites.
## Requirements
### Requirement: AgentBus trait shall define the interface for agent-to-agent communication

AgentBus SHALL define a trait with register(), send(), broadcast(), and subscribe() methods for inter-agent communication.

#### Scenario: AgentBus trait definition
- **WHEN** A component needs to communicate between agents
- **THEN** It SHALL use the AgentBus trait as the abstraction
- **AND** Any implementation (Memory, File, MessageProxy) SHALL be interchangeable via this trait

#### Scenario: AgentBus send operation
- **WHEN** An agent calls `send(to, payload)`
- **THEN** The message SHALL be delivered to the specified recipient
- **AND** Result SHALL be `Result<(), BusError>`

#### Scenario: AgentBus broadcast operation
- **WHEN** An agent calls `broadcast(recipients, payload)`
- **THEN** The message SHALL be delivered to all recipients
- **AND** Return SHALL be `Result<usize, BusError>` with delivery count

---

### Requirement: MemoryAgentBus shall provide in-process agent communication

MemoryAgentBus SHALL implement AgentBus using in-memory data structures for communication between agents in the same process.

#### Scenario: MemoryAgentBus registration
- **WHEN** Agent calls `register(agent_id)` on MemoryAgentBus
- **THEN** The agent SHALL be registered in an internal registry
- **AND** Agent SHALL be discoverable for subsequent send/broadcast calls

#### Scenario: MemoryAgentBus send
- **WHEN** Agent A sends to Agent B via MemoryAgentBus
- **AND** Agent B is subscribed
- **THEN** Message SHALL be delivered immediately to Agent B's subscription stream

#### Scenario: MemoryAgentBus no subscriber
- **WHEN** Agent A sends to Agent B
- **AND** Agent B is not subscribed
- **THEN** The send SHALL return `Ok(())` (at-most-once semantics)
- **AND** Message SHALL be dropped

---

### Requirement: FileAgentBus shall provide file-based agent communication

FileAgentBus SHALL implement AgentBus using the filesystem for communication between agents in different processes on the same machine.

#### Scenario: FileAgentBus file structure
- **WHEN** FileAgentBus is created with a base path
- **THEN** It SHALL use `{base_path}/{agent_id}/inbound/` for incoming messages
- **AND** Use `{base_path}/{agent_id}/control/` for registration signals

#### Scenario: FileAgentBus subscription
- **WHEN** Agent calls `subscribe()` on FileAgentBus
- **THEN** It SHALL monitor the inbound directory for new message files
- **AND** Yield messages as they appear

---

### Requirement: MessageProxyAgentBus shall adapt MessageProxy to AgentBus

MessageProxyAgentBus SHALL wrap the existing MessageProxy gRPC client to implement the AgentBus trait.

#### Scenario: MessageProxy integration
- **WHEN** MessageProxyAgentBus is created with a socket path
- **THEN** It SHALL connect to the MessageProxy server via UDS
- **AND** Implement all AgentBus methods using gRPC calls

#### Scenario: MessageProxy registration
- **WHEN** Agent calls `register(agent_id)` on MessageProxyAgentBus
- **THEN** It SHALL call `MessageProxyServiceClient::register()`
- **AND** Handle success/failure appropriately

---

### Requirement: BusMessage shall represent inter-agent messages

BusMessage SHALL contain id, from, to, payload, and timestamp fields.

#### Scenario: BusMessage structure
- **WHEN** Agent sends a message via AgentBus
- **THEN** The message SHALL include a unique id
- **AND** from and to agent identifiers
- **AND** arbitrary payload bytes
- **AND** timestamp in milliseconds

---

### Requirement: BusError shall represent communication failures

BusError SHALL be an enum that represents various failure modes for agent communication.

#### Scenario: BusError variants
- **WHEN** AgentBus operation fails
- **THEN** Error SHALL be one of: NotRegistered, NotConnected, SendFailed, SubscribeFailed
- **AND** Each SHALL contain a descriptive message

