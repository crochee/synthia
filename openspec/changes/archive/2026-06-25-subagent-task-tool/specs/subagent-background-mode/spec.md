## ADDED Requirements

### Requirement: The `task` tool SHALL expose the `background` parameter only when `AgentControl` is available

The JSON schema for the `task` tool SHALL include the `background` parameter only when the agent runtime has been configured with an `AgentControl`. When `AgentControl` is absent, the parameter SHALL be omitted from the schema.

#### Scenario: Server path with AgentControl
- **WHEN** `AgentRunConfig.agent_control` is `Some`
- **THEN** the `task` tool schema SHALL include `background`

#### Scenario: Standalone path without AgentControl
- **WHEN** `AgentRunConfig.agent_control` is `None`
- **THEN** the `task` tool schema SHALL NOT include `background`

---

### Requirement: Background tasks SHALL be registered with `AgentControl`

When `background` is true, the `task` tool SHALL spawn the subagent and register the resulting async handle with `AgentControl::register_background_task` without awaiting completion.

#### Scenario: Launch background task
- **WHEN** the LLM calls `task` with `background: true`
- **THEN** the tool SHALL return immediately and the subagent SHALL continue executing in the background

---

### Requirement: The main loop SHALL poll for completed background tasks on each iteration

At the start of each iteration, the main loop SHALL call `AgentControl::check_completed` and inject a result message for every completed task into `ctx.messages`.

#### Scenario: Background task completes between iterations
- **WHEN** a background task finishes while the parent loop is running
- **THEN** the next iteration SHALL inject the task result into the parent context

---

### Requirement: Background task results SHALL be injected as structured XML

Completed background tasks SHALL be injected into the parent context as a message containing:

```xml
<task id="{session_id}" state="{completed|error}">
<summary>{summary text}</summary>
<task_result>{subagent output}</task_result>
</task>
```

If the task failed, `<task_result>` SHALL be replaced with `<task_error>`.

#### Scenario: Successful background task
- **WHEN** a background task completes successfully
- **THEN** the injected message SHALL contain `<task state="completed">` with the subagent output

#### Scenario: Failed background task
- **WHEN** a background task fails
- **THEN** the injected message SHALL contain `<task state="error">` with the error details
