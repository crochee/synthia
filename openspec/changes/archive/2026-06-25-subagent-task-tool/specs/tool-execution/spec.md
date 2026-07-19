## MODIFIED Requirements

### Requirement: Tool execution SHALL enforce configurable timeouts

Each tool category SHALL have a default timeout. Tool execution SHALL be wrapped in tokio::time::timeout with the configured value. When timeout is exceeded, the tool call SHALL be cancelled and return a timeout error. The `subagent` tool category SHALL have a dedicated timeout of 300 seconds by default.

#### Scenario: Subagent task times out
- **WHEN** a foreground `task` tool call runs longer than the configured subagent timeout
- **THEN** the tool call SHALL be cancelled and return a timeout error

#### Scenario: Shell command times out
- **WHEN** a bash command runs longer than 60 seconds (default shell timeout)
- **THEN** the tool call SHALL be cancelled and return a timeout error

---

## ADDED Requirements

### Requirement: `build_default_tool_registry` SHALL accept optional subagent dependencies

The `build_default_tool_registry` function SHALL accept optional `AgentControl` and `SubagentSessionFactory` parameters. When both are provided, it SHALL register the `task` tool; when either is missing, it SHALL omit the `task` tool.

#### Scenario: Full subagent infrastructure available
- **WHEN** `build_default_tool_registry` is called with both `AgentControl` and `SubagentSessionFactory`
- **THEN** the returned registry SHALL contain the `task` tool

#### Scenario: Subagent infrastructure unavailable
- **WHEN** `build_default_tool_registry` is called without `AgentControl` or `SubagentSessionFactory`
- **THEN** the returned registry SHALL NOT contain the `task` tool

---

### Requirement: Tool registry construction SHALL remain backward compatible for callers without subagent infrastructure

Existing callers of `build_default_tool_registry` that pass only the workspace root SHALL continue to receive a registry with the basic tool set and without the `task` tool.

#### Scenario: Legacy caller uses old signature
- **WHEN** an existing call site invokes `build_default_tool_registry(workspace_root)`
- **THEN** the call SHALL compile and the returned registry SHALL NOT contain the `task` tool
