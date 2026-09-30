## MODIFIED Requirements

### Requirement: Single AgentEvent Definition
The system SHALL have exactly one canonical `AgentEvent` definition at `synthia-agent/src/types/event.rs`. All other definitions SHALL be removed or redirected to the canonical definition.

#### Scenario: Canonical AgentEvent is used
- **WHEN** any part of the system emits an agent event
- **THEN** it SHALL use the canonical `AgentEvent` enum from `types/event.rs`

#### Scenario: No duplicate AgentEvent definitions exist
- **WHEN** searching for `AgentEvent` definitions across the codebase
- **THEN** only one definition SHALL exist (at `types/event.rs`)

---

### Requirement: Event Type Coverage
The canonical `AgentEvent` enum SHALL cover all event types needed by the agent runtime including tool calls, decisions, errors, context updates, and session state changes.

#### Scenario: All event variants are accessible
- **WHEN** code needs to handle any agent event type
- **THEN** all variants SHALL be accessible from the canonical definition

#### Scenario: Event variants are exhaustive
- **WHEN** adding a new event type to the agent
- **THEN** it SHALL be added to the canonical `AgentEvent` enum