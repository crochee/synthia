## MODIFIED Requirements

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

## ADDED Requirements

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
