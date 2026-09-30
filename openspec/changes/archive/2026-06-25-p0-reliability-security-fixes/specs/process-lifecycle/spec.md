<!--
Delta spec for process-lifecycle capability.
-->

## ADDED Requirements

### Requirement: Process Group Creation

The bash tool SHALL create a new process group for each command execution using `Command::process_group(0)`, ensuring that all child and grandchild processes can be terminated as a unit.

#### Scenario: bash command spawns child process

- **WHEN** the bash tool executes `bash -c "sleep 1000 &"`
- **THEN** a new process group SHALL be created with the child process as group leader
- **AND** the grandchild `sleep` process SHALL be in the same process group

---

### Requirement: Process Group Termination on Timeout

When a bash command times out or is cancelled, the system SHALL terminate the entire process group using `killpg(SIGTERM)`, wait up to 3 seconds, then escalate to `killpg(SIGKILL)` if the group is still alive.

#### Scenario: command times out with grandchild process

- **WHEN** a bash command times out after the configured timeout
- **AND** the command spawned a grandchild process (e.g., `sleep 1000 &`)
- **THEN** the system SHALL send SIGTERM to the entire process group
- **AND** wait up to 3 seconds for processes to exit
- **AND** send SIGKILL to the process group if any processes remain
- **AND** no orphan processes SHALL remain

---

### Requirement: IO Drain After Kill

After terminating the process group, the system SHALL drain remaining IO output for up to 2 seconds before returning, to prevent pipe buffer corruption.

#### Scenario: process killed during output

- **WHEN** a process is killed while writing to stdout/stderr
- **THEN** the system SHALL drain remaining pipe output for up to 2 seconds
- **AND** the drained output SHALL be included in the tool result (truncated if necessary)

---

### Requirement: Cancellation Token Integration

The process group termination SHALL be triggered by both timeout and cancellation token, ensuring user-initiated cancellation also cleans up child processes.

#### Scenario: user cancels during command execution

- **WHEN** the cancellation token is triggered during bash command execution
- **THEN** the system SHALL terminate the process group (SIGTERM → SIGKILL)
- **AND** return `ToolExecutionError::Cancelled`
- **AND** no orphan processes SHALL remain
