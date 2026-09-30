## ADDED Requirements

### Requirement: The system SHALL expose a `task` tool when both `AgentControl` and `SubagentSessionFactory` are available

The `task` tool SHALL be registered in the tool registry only when the agent runtime has been configured with both an `AgentControl` instance and a `SubagentSessionFactory`. If either dependency is missing, the tool SHALL NOT appear in the tool list exposed to the LLM.

#### Scenario: Server-managed session with full infrastructure
- **WHEN** `AgentRunConfig` contains both `agent_control` and `subagent_session_factory`
- **THEN** `build_default_tool_registry` SHALL register the `task` tool

#### Scenario: Standalone agent without subagent infrastructure
- **WHEN** `AgentRunConfig` lacks `agent_control` or `subagent_session_factory`
- **THEN** `build_default_tool_registry` SHALL NOT register the `task` tool

---

### Requirement: The `task` tool SHALL accept an Opencode-aligned parameter schema

The `task` tool SHALL accept the following parameters:
- `description`: a short (3-5 words) description of the task (required)
- `prompt`: the detailed task for the subagent to perform (required)
- `subagent_type`: the type of specialized agent to use (required)
- `background`: whether to run the subagent asynchronously (optional, default false)
- `task_id`: an identifier used to resume a previous task session (optional)

#### Scenario: LLM invokes task with required parameters
- **WHEN** the LLM calls `task` with `description`, `prompt`, and `subagent_type`
- **THEN** the system SHALL spawn or resume a subagent session

#### Scenario: LLM omits required parameters
- **WHEN** the LLM calls `task` without `description`, `prompt`, or `subagent_type`
- **THEN** the system SHALL return a validation error

---

### Requirement: The `task` tool SHALL resume an existing subagent session when `task_id` is provided

When `task_id` is provided, the `task` tool SHALL continue the referenced subagent session instead of creating a new one. If the referenced session does not exist or is not accessible to the current user, the tool SHALL return an error.

#### Scenario: Resume a previous task
- **WHEN** the LLM calls `task` with a valid `task_id`
- **THEN** the existing subagent session SHALL receive the new prompt and continue execution

#### Scenario: Resume a non-existent task
- **WHEN** the LLM calls `task` with a `task_id` that does not exist
- **THEN** the system SHALL return an error indicating the task was not found

---

### Requirement: Foreground task execution SHALL return the subagent result to the caller

When `background` is false or absent, the `task` tool SHALL wait for the subagent to complete and SHALL return its final output as the tool result.

#### Scenario: Foreground task completes successfully
- **WHEN** the LLM calls `task` with `background: false`
- **THEN** the tool result SHALL contain the subagent's final output

#### Scenario: Foreground task fails
- **WHEN** the subagent encounters an error during foreground execution
- **THEN** the tool result SHALL contain an error message and the tool SHALL report failure
