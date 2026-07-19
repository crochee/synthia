## ADDED Requirements

### Requirement: AgentTool SHALL execute sub-agents in a ReAct loop

When the AgentTool is invoked, the system SHALL spawn a sub-agent thread that runs the full StreamBuilder ReAct execution loop (LLM sampling → tool execution → result collection), rather than returning a placeholder text.

#### Scenario: Foreground sub-agent execution
- **WHEN** AgentTool is called with `background: false` (default) and a valid task description
- **THEN** the system SHALL spawn a tokio task running the sub-agent's ReAct loop, await its completion, and return the sub-agent's final text output as the tool result

#### Scenario: Background sub-agent execution
- **WHEN** AgentTool is called with `background: true` and a valid task description
- **THEN** the system SHALL spawn a tokio task running the sub-agent's ReAct loop, immediately return a running status indicator, and inject the sub-agent's final result as a synthetic user message into the parent session when the sub-agent completes

#### Scenario: Empty task description
- **WHEN** AgentTool is called with an empty or missing task description
- **THEN** the system SHALL return an error indicating that a task parameter is required

### Requirement: Sub-agent config SHALL inherit from parent runtime state

When spawning a sub-agent, the system SHALL inherit the parent's active runtime configuration (model, provider, token budget, permission policy) as the sub-agent's starting configuration, rather than using default or persisted config values.

#### Scenario: Model inheritance
- **WHEN** a sub-agent is spawned and no explicit model override is specified
- **THEN** the sub-agent SHALL use the same model and provider as the parent agent's current turn

#### Scenario: Permission inheritance
- **WHEN** a sub-agent is spawned
- **THEN** the sub-agent's permission policy SHALL be derived from the parent's policy, downgraded to the User privilege layer, with sub-agent-specific tool restrictions applied

### Requirement: Sub-agent SHALL apply ForkPolicy to filter conversation history

When spawning a sub-agent, the system SHALL apply the configured ForkPolicy to determine which conversation history messages the sub-agent inherits from the parent session.

#### Scenario: SystemOnly fork (default)
- **WHEN** a sub-agent is spawned with default ForkPolicy::SystemOnly
- **THEN** the sub-agent SHALL receive only system messages and tool definitions from the parent, with no conversation history

#### Scenario: InheritAll fork
- **WHEN** a sub-agent is spawned with ForkPolicy::InheritAll
- **THEN** the sub-agent SHALL receive the full parent conversation history

### Requirement: Sub-agent depth SHALL be limited

The system SHALL enforce a maximum sub-agent spawning depth to prevent infinite recursive agent creation.

#### Scenario: Depth limit exceeded
- **WHEN** a sub-agent with depth equal to the configured maximum (default: 1) attempts to spawn another sub-agent
- **THEN** the spawn SHALL fail with an error indicating the depth limit has been reached

### Requirement: Sub-agent concurrency SHALL be limited

The system SHALL limit the maximum number of concurrently executing sub-agents to prevent resource exhaustion.

#### Scenario: Concurrency limit reached
- **WHEN** the number of currently executing sub-agents equals the configured maximum (default: 6)
- **THEN** new sub-agent spawn requests SHALL fail with an error indicating the concurrency limit has been reached

### Requirement: AgentInstance types SHALL be unified

The system SHALL maintain a single canonical AgentInstance type that contains all fields required for sub-agent execution, eliminating the current duplication between registry::instance::AgentInstance and tools::agent_tools::coordinator::AgentInstance.

#### Scenario: Single AgentInstance type
- **WHEN** any code creates or references an AgentInstance
- **THEN** it SHALL use the unified type, with old type paths preserved as re-export shims