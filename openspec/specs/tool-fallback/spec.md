# tool-fallback Specification

## Purpose
TBD - created by archiving change error-recovery-cascade. Update Purpose after archive.
## Requirements
### Requirement: Consecutive tool failures SHALL trigger fallback

When the same tool fails 2 consecutive times within a session, the system SHALL attempt to provide a fallback response instead of escalating to higher recovery levels.

#### Scenario: Same tool fails twice triggers fallback
- **WHEN** tool "web_fetch" fails once, then fails again
- **THEN** the system SHALL query FallbackProvider for a fallback strategy
- **AND** if a fallback exists, the fallback message SHALL be returned as the tool result

#### Scenario: Different tool failures do not count as consecutive
- **WHEN** tool "web_fetch" fails, then tool "bash" fails
- **THEN** no fallback SHALL be triggered
- **AND** each failure SHALL be counted separately

#### Scenario: Successful execution resets consecutive failure counter
- **WHEN** tool "web_fetch" fails once
- **AND** then tool "web_fetch" succeeds
- **THEN** the consecutive failure counter for "web_fetch" SHALL be reset to zero

---

### Requirement: Fallback messages SHALL be informative

Fallback responses SHALL clearly communicate that the original tool is unavailable and provide guidance.

#### Scenario: Web fetch fallback includes cached content mention
- **WHEN** web_fetch fallback is triggered
- **THEN** the fallback message SHALL contain "cached content" or "network unavailable"
- **AND** the message SHALL be formatted as a tool result

#### Scenario: Bash fallback includes simplified alternative
- **WHEN** bash fallback is triggered
- **THEN** the fallback message SHALL indicate the command cannot be executed
- **AND** the message SHALL suggest an alternative approach

#### Scenario: Unknown tool without fallback escalates
- **WHEN** a tool with no registered fallback fails twice consecutively
- **THEN** the failure SHALL escalate to L4 (Auto-Compact)

---

### Requirement: Fallback execution SHALL be counted as success

When a fallback message is returned, the system SHALL call `record_success()` to reset the error counter.

#### Scenario: Fallback resets error counter
- **WHEN** fallback is returned for a consecutive tool failure
- **THEN** the consecutive error counter SHALL be reset
- **AND** subsequent tool executions SHALL start fresh

