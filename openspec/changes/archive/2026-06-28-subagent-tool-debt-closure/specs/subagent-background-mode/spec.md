<!--
Delta spec for modified capability: subagent-background-mode
Adds requirement for background completion notification via parent_event_sender.
-->

## ADDED Requirements

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
