## Purpose

Define the session model for subagents, including parent-child relationships, namespace isolation, and lifecycle management for sessions spawned by the AgentTool.
## Requirements
### Requirement: Subagent sessions SHALL be created with a parent_id referencing the spawning session

When `AgentTool` spawns a subagent, the resulting child session metadata SHALL contain `parent_id` equal to the parent session id.

#### Scenario: Tool spawns a subagent
- **WHEN** a parent session invokes `AgentTool` to spawn a subagent
- **THEN** a new child session is created with `parent_id` set to the parent session id

---

### Requirement: Subagent sessions SHALL reside in the same user namespace as the parent

The child session directory SHALL be located under `{sessions_root}/{user_id}/{child_session_id}/`, identical to a regular session, with the parent relationship tracked only in metadata.

#### Scenario: Child session storage layout
- **WHEN** a subagent session is created for user `u1`
- **THEN** its files are stored under the `u1` user directory

---

### Requirement: Session metadata loading SHALL remain backward compatible when parent_id is absent

Existing session metadata files that do not contain a `parent_id` field SHALL deserialize with `parent_id` defaulting to `None`.

#### Scenario: Load legacy metadata
- **WHEN** an old session metadata file without `parent_id` is loaded
- **THEN** the resulting `SessionMetadata` has `parent_id == None`

---

### Requirement: AgentTool SHALL create child sessions through the injected SubagentSessionFactory

`AgentTool` SHALL NOT construct child sessions directly; it SHALL use the `SubagentSessionFactory` provided in `AgentRunConfig`. When creating the child session, `AgentTool` SHALL derive the subagent's permission set from the parent session using `derive_subagent_permission`, and SHALL apply the configured `ForkPolicy` to filter inherited message history.

#### Scenario: Factory is configured
- **WHEN** `AgentRunConfig` contains a `SubagentSessionFactory`
- **THEN** `AgentTool` calls `create_child` on the factory to obtain a child session handle
- **AND THEN** the child's initial permissions and message history reflect the derived permission set and fork policy

#### Scenario: Factory is absent
- **WHEN** `AgentRunConfig` does not contain a `SubagentSessionFactory`
- **THEN** `AgentTool` SHALL return an error indicating subagent sessions are unavailable

---

### Requirement: `build_subagent_config` SHALL apply `ForkPolicy` to inherited message history

The `build_subagent_config` function SHALL accept the parent's message history and configured `ForkPolicy` and SHALL produce a child configuration whose initial messages are filtered according to the policy.

#### Scenario: ForkPolicy is LastNTurns
- **WHEN** `ForkPolicy::LastNTurns(2)` is configured
- **THEN** the child session SHALL inherit only the last two user-assistant turns from the parent

#### Scenario: ForkPolicy is Empty
- **WHEN** `ForkPolicy::Empty` is configured
- **THEN** the child session SHALL start with no inherited conversation history

---

### Requirement: `build_subagent_config` SHALL apply derived permissions to the child approval service

The child session configuration produced by `build_subagent_config` SHALL use the permission set returned by `derive_subagent_permission` so that tool orchestration and approval checks in the child are bounded by those rules.

#### Scenario: Child permission set differs from parent
- **WHEN** a subagent is spawned
- **THEN** the child's `ApprovalService` or permission rules SHALL reflect the derived deny-only inheritance and default-deny recursion rules

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

