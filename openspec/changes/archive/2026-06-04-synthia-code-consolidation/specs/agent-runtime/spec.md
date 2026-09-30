## MODIFIED Requirements

### Requirement: ReAct Loop Implementation
The ReAct loop SHALL be implemented in `synthia-agent/src/agent/react.rs` as the canonical implementation. The loop SHALL support reasoning and acting in cycles, maintaining state across iterations.

#### Scenario: ReAct loop executes one cycle
- **WHEN** the agent executes one ReAct cycle
- **THEN** it SHALL produce a thought, an action, and observe the result

#### Scenario: ReAct loop maintains state across cycles
- **WHEN** the agent executes multiple ReAct cycles
- **THEN** the state SHALL be maintained and updated with each iteration

---

### Requirement: Agent Event Types
The agent SHALL emit events for significant state transitions including tool calls, decisions, errors, and context changes.

#### Scenario: Tool call event emitted
- **WHEN** the agent calls a tool
- **THEN** a `ToolCall` event SHALL be emitted with tool name and parameters

#### Scenario: Decision event emitted
- **WHEN** the agent makes a decision
- **THEN** a `Decision` event SHALL be emitted with reasoning

#### Scenario: Error event emitted
- **WHEN** an error occurs during agent execution
- **THEN** an `Error` event SHALL be emitted with error details

---

### Requirement: Agent Configuration Hierarchy
The agent configuration SHALL support a three-layer hierarchy: CLI YAML configuration, Server configuration, and Agent Runtime configuration.

#### Scenario: CLI config parses successfully
- **WHEN** CLI provides YAML configuration
- **THEN** it SHALL be parsed and converted to Server configuration via `From`/`Into`

#### Scenario: Server config merges correctly
- **WHEN** Server configuration is loaded
- **THEN** it SHALL be merged and converted to Agent Runtime configuration

#### Scenario: Runtime config drives agent behavior
- **WHEN** Agent Runtime configuration is available
- **THEN** the agent SHALL use it to configure all runtime behaviors