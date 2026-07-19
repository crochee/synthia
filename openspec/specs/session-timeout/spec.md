# session-timeout Specification

## Purpose
TBD - created by archiving change p0-reliability-security-fixes. Update Purpose after archive.
## Requirements
### Requirement: Session Wall-Clock Timeout

The agent loop SHALL enforce a session-level wall-clock timeout, configurable via `AgentConfig.session_wall_clock_timeout` with a default of 30 minutes. When the session duration exceeds this timeout, `should_stop` SHALL return true.

#### Scenario: session exceeds wall-clock timeout

- **WHEN** the session has been running for longer than `session_wall_clock_timeout`
- **THEN** `LoopContext::should_stop` SHALL return true
- **AND** the session SHALL end with `SessionEndReason::Timeout`

---

### Requirement: Timeout Configurability

The session wall-clock timeout SHALL be configurable through `AgentConfig`, allowing users to disable it by setting it to `None` or `Duration::ZERO`.

#### Scenario: timeout disabled

- **WHEN** `AgentConfig.session_wall_clock_timeout` is set to `None`
- **THEN** `should_stop` SHALL NOT check wall-clock time
- **AND** only `max_iterations` SHALL limit the session

#### Scenario: custom timeout

- **WHEN** `AgentConfig.session_wall_clock_timeout` is set to `Duration::from_secs(300)`
- **THEN** the session SHALL stop after 5 minutes of wall-clock time

---

### Requirement: Session Start Time Tracking

`LoopContext` SHALL track the session start time using `std::time::Instant`, recorded when the loop begins.

#### Scenario: session start time recorded

- **WHEN** the agent loop starts
- **THEN** `LoopContext` SHALL record the current `Instant` as the session start time
- **AND** this value SHALL remain constant for the duration of the session

---

### Requirement: Timeout Warning Event

Before the wall-clock timeout is reached, the system SHALL emit a warning event when 80% of the timeout duration has elapsed.

#### Scenario: 80% timeout warning

- **WHEN** 80% of `session_wall_clock_timeout` has elapsed
- **THEN** the system SHALL emit a `Warning` event with message "Session approaching wall-clock timeout"
- **AND** the session SHALL continue running

