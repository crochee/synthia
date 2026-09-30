## ADDED Requirements

### Requirement: Agent SHALL preserve and replay checkpoint state on resume

When `Agent::resume(session_id)` is called, the system SHALL load the latest checkpoint for the session, restore the full message history and iteration counter, and continue execution from the exact point where the session was interrupted.

#### Scenario: Resume replays message history
- **WHEN** `Agent::resume("session-123")` is called with a checkpoint containing 50 messages and iteration counter 23
- **THEN** the agent SHALL start the next iteration at iteration 23 with all 50 messages in context

### Requirement: Resume SHALL restore iteration counter correctly

The iteration counter SHALL resume from the checkpointed `start_iteration` value and increment normally from there, not reset to zero.

#### Scenario: Iteration counter continues from checkpoint
- **WHEN** a session resumes from a checkpoint at iteration 47
- **THEN** the next emitted `IterationStarted` event SHALL have `iteration = 48`

### Requirement: Resume with no checkpoint SHALL fall back to session store

If no checkpoint file exists for the session, the system SHALL load messages from the session JSONL store and start from iteration 0.

#### Scenario: Fallback to session store
- **WHEN** `Agent::resume("session-456")` is called but no checkpoint exists
- **THEN** the system SHALL load messages from the session JSONL store
- **AND** start from iteration 0