# subagent-background-mode Specification

## Purpose
TBD - created by archiving change subagent-task-tool. Update Purpose after archive.
## Requirements
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

### Requirement: Background subagent completion SHALL notify parent via SubagentCompleted event

When a background subagent completes (successfully or with error), the `SubagentSessionFactory` SHALL emit an `AgentEvent::SubagentEvent` with inner `SubagentCompleted { session_id, result_summary }` to the parent's event stream via `ChildSessionHandle.parent_event_sender`. The `result_summary` SHALL be the first 500 characters of the subagent's final output or error message.

#### Scenario: Background task completes successfully
- **WHEN** a background subagent finishes with a successful result
- **THEN** `SubagentCompleted { session_id, result_summary }` SHALL be sent to `parent_event_sender`
- **AND** `result_summary` SHALL contain the first 500 characters of the output

#### Scenario: Background task fails
- **WHEN** a background subagent finishes with an error
- **THEN** `SubagentCompleted { session_id, result_summary }` SHALL be sent to `parent_event_sender`
- **AND** `result_summary` SHALL contain the first 500 characters of the error message

#### Scenario: Parent event sender is closed
- **WHEN** the background subagent completes but `parent_event_sender` is closed (parent session ended)
- **THEN** the send SHALL be a no-op (best-effort)
- **AND** no error SHALL be propagated to the background subagent

#### Scenario: Result summary truncated at character boundary
- **WHEN** the subagent output is longer than 500 characters
- **THEN** `result_summary` SHALL be truncated at a valid UTF-8 character boundary at or before 500 characters
- **AND** a truncation indicator SHALL be appended if truncation occurred

