# agent-tool-wiring Specification

## Purpose
TBD - created by archiving change agent-toolification-v3. Update Purpose after archive.
## Requirements
### Requirement: AgentTool Factory Wiring

The `synthia-tools` crate MUST expose a function or method that constructs an `AgentTool` from a `ToolRegistry` reference and registers it into the registry. The function MUST be invoked during `Agent::builder().build()` so that `AgentTool` is part of the default tool set without user action.

#### Scenario: Auto-Registration on Agent Build

- **WHEN** `Agent::builder().build()` is called
- **THEN** the agent's internal `ToolRegistry` MUST contain an entry with `name == "agent"`

#### Scenario: Registry Has AgentTool Before First Run

- **WHEN** a user inspects `Agent::registry()` before calling `agent.run()`
- **THEN** the registry MUST contain `AgentTool` (entry named `"agent"`)

### Requirement: AgentTool Trait Stable

The `AgentTool` implementation MUST remain unchanged in its public API. Only the wiring (factory invocation) is added by this capability.

#### Scenario: Existing AgentTool Tests Pass

- **WHEN** existing tests for `AgentTool` are executed
- **THEN** they MUST pass without modification

