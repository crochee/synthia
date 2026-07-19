# session-reset Specification

## Purpose
Complete session reconstruction when error recovery exhausts all levels, resetting the agent to a clean state.

## ADDED Requirements

### Requirement: L5 escalation SHALL trigger session reset

When error recovery escalates to L5 (after L4 compact failure), the system SHALL perform a session reset with scope `Conversation`.

#### Scenario: L5 reset discards current context
- **WHEN** L5 (Reset) is triggered
- **THEN** the current context messages SHALL be discarded
- **AND** a new conversation SHALL begin with the same user input

#### Scenario: L5 reset preserves session metadata
- **WHEN** L5 (Reset) is triggered
- **THEN** session ID, user preferences, and HotMemory SHALL be preserved
- **AND** only the conversation messages SHALL be reset

---

### Requirement: Loop detector SHALL be reset on L5

When L5 reset is triggered, all loop detection state SHALL be cleared.

#### Scenario: Loop detector state is cleared
- **WHEN** L5 (Reset) is triggered
- **THEN** `LoopDetectorSet::reset()` SHALL be called
- **AND** all detector state SHALL be cleared to initial values

#### Scenario: Circuit breaker is reset
- **WHEN** L5 (Reset) is triggered
- **THEN** the circuit breaker counter SHALL be reset to zero
- **AND** the circuit SHALL be closed

---

### Requirement: Steering channel SHALL be drained on L5

When L5 reset is triggered, all pending steering messages SHALL be discarded.

#### Scenario: Steering channel is drained
- **WHEN** L5 (Reset) is triggered
- **THEN** the steering channel SHALL be drained
- **AND** no pending steering messages SHALL be carried over to the new session

---

### Requirement: Error counter SHALL be reset after L5

After L5 reset completes successfully, the consecutive error counter SHALL be reset to zero.

#### Scenario: Reset clears error counter
- **WHEN** L5 reset completes successfully
- **THEN** `consecutive_errors` SHALL be set to zero
- **AND** `record_success()` SHALL be called
- **AND** the agent SHALL resume normal operation

#### Scenario: Reset failure enters cooldown
- **WHEN** L5 reset itself fails
- **THEN** the system SHALL enter fail-fast mode
- **AND** a 30-second cooldown period SHALL begin
- **AND** no further recovery attempts SHALL be made during cooldown
