## ADDED Requirements

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

`AgentTool` SHALL NOT construct child sessions directly; it SHALL use the `SubagentSessionFactory` provided in `AgentRunConfig`.

#### Scenario: Factory is configured
- **WHEN** `AgentRunConfig` contains a `SubagentSessionFactory`
- **THEN** `AgentTool` calls `create_child` on the factory to obtain a child session handle

#### Scenario: Factory is absent
- **WHEN** `AgentRunConfig` does not contain a `SubagentSessionFactory`
- **THEN** `AgentTool` SHALL return an error indicating subagent sessions are unavailable
