# Spec: a2a-transport

## ADDED Requirements

### Requirement: synthia-a2a crate
The system SHALL provide a `synthia-a2a` crate that depends on `a2a-lf`, `a2a-client-lf`, and `a2a-server-lf`, exposing `A2aTransport`, `SynthiaA2aHandler`, and `AgentCard` construction utilities.

#### Scenario: crate provides a2a transport types
- **WHEN** a consumer adds `synthia-a2a` as a dependency
- **THEN** `A2aTransport`, `SynthiaA2aHandler`, and `AgentCard` are publicly available from the crate root

### Requirement: A2aTransport struct
`A2aTransport` SHALL hold:
- `server: Option<A2aServer>` — exposes this agent for remote invocation
- `client_registry: DashMap<String, A2aClient>` — cache of discovered remote agent clients
- `card: AgentCard` — this agent's capability card

#### Scenario: transport holds server client and card
- **WHEN** an `A2aTransport` instance is created
- **THEN** it contains an optional server, a client registry, and an agent card

### Requirement: A2aTransport.from_handle
`A2aTransport` SHALL be constructable from an `AgentHandle`:
- `AgentCard.name = handle.id`
- `AgentCard.skills = handle.tool_registry` tool list
- `AgentCard.capabilities.streaming = true`

#### Scenario: build transport from agent handle
- **WHEN** `A2aTransport::from_handle(handle)` is called
- **THEN** the resulting transport's card name matches `handle.id`, its skills list the handle's tools, and streaming is enabled

### Requirement: A2aTransport.serve
`A2aTransport` SHALL start an A2A Server so other agents can discover and invoke this agent via the A2A protocol. `SynthiaA2aHandler` SHALL bridge `on_send_message` to `handle.run` and `on_send_streaming_message` to `handle.run_stream`.

#### Scenario: serve exposes agent over a2a
- **WHEN** `transport.serve()` is called on an `A2aTransport` with a configured server
- **THEN** remote agents can invoke this agent via the A2A protocol and responses are produced by `handle.run` / `handle.run_stream`

### Requirement: A2aTransport.discover
`A2aTransport` SHALL discover remote agents by fetching `/.well-known/agent.json` and caching the resulting `A2aClient` in the client registry.

#### Scenario: discover remote agent
- **WHEN** `transport.discover(agent_url)` is called
- **THEN** the agent card at `agent_url/.well-known/agent.json` is fetched and a corresponding `A2aClient` is stored in the client registry

### Requirement: InMemoryMessageBus removal
The system SHALL remove `InMemoryMessageBus` and the `MessageBus` trait. All inter-agent communication SHALL go through the A2A protocol.

#### Scenario: message bus removed
- **WHEN** the codebase is compiled after this change
- **THEN** `InMemoryMessageBus` and `MessageBus` no longer exist and all agent communication uses A2A
