<!--
Delta spec for modified capability: subagent-session-model
Adds requirement for depth tracking and max_depth enforcement.
-->

## ADDED Requirements

### Requirement: SubagentConfig SHALL track spawn depth and enforce max_depth limit

The `SubagentConfig` SHALL include a `depth: usize` field indicating the spawn depth of the subagent (root agent has depth 0, direct children have depth 1, etc.). The `SubagentSessionFactory::create_child` method SHALL accept the parent's depth and set the child's depth to `parent_depth + 1`. The `AgentTool::call` method SHALL check `config.depth >= manager.max_depth()` before spawning and SHALL return `ToolOutput::error("Max sub-agent depth reached")` if the limit is exceeded.

#### Scenario: Root agent spawns direct child
- **WHEN** the root agent (depth 0) calls `AgentTool` to spawn a subagent
- **THEN** the child's `SubagentConfig.depth` SHALL be 1

#### Scenario: Depth limit exceeded
- **WHEN** `max_depth = 3` and a subagent at depth 3 attempts to spawn another child
- **THEN** `AgentTool::call` SHALL return `ToolOutput::error("Max sub-agent depth reached")`
- **AND** no child session SHALL be created

#### Scenario: Depth limit not exceeded
- **WHEN** `max_depth = 3` and a subagent at depth 2 attempts to spawn another child
- **THEN** the child SHALL be created with `depth = 3`
- **AND** the spawn SHALL succeed

---

### Requirement: SubagentManager::current_depth SHALL return real depth instead of stub

The `SubagentManager::current_depth()` method SHALL return the actual depth from the current `SubagentConfig` instead of the stub value `0`. The method SHALL read `self.config.depth` (or equivalent runtime state) to provide an accurate depth value.

#### Scenario: current_depth returns config depth
- **WHEN** `current_depth()` is called on a subagent at depth 2
- **THEN** the return value SHALL be 2

#### Scenario: Root agent current_depth is zero
- **WHEN** `current_depth()` is called on the root agent
- **THEN** the return value SHALL be 0
