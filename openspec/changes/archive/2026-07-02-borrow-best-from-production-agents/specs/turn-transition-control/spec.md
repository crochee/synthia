## ADDED Requirements

### Requirement: TurnTransition SHALL use Result<_, ControlFlow> defect channel

Turn transitions MUST return `Result<TurnOutput, ControlFlow<TurnTransition>>` where `ControlFlow::Continue(TurnTransition)` represents a recoverable defect (e.g., context overflow requiring compaction) and `ControlFlow::Break(TurnTransition)` represents an unrecoverable defect. The outer loop MUST match on `ControlFlow` to handle defects, equivalent to opencode's `catchDefect` semantics.

#### Scenario: Recoverable defect triggers compaction retry

- **WHEN** a turn returns `ControlFlow::Continue(TurnTransition::ContextOverflow)`
- **THEN** the outer loop runs context compaction
- **AND** retries the turn with the compacted context
- **AND** the retry count is incremented

#### Scenario: Unrecoverable defect terminates turn

- **WHEN** a turn returns `ControlFlow::Break(TurnTransition::FatalError(err))`
- **THEN** the outer loop terminates the turn
- **AND** propagates the error to the session
- **AND** no retry is attempted

---

### Requirement: Defect retry SHALL be capped at 3 attempts

The outer defect handler MUST retry a recoverable defect at most 3 times within a single turn. After the 3rd retry, the defect MUST be converted to `ControlFlow::Break` and propagated as an error. This prevents infinite retry loops.

#### Scenario: Third retry succeeds

- **WHEN** a turn fails with `ContextOverflow` twice and the third retry succeeds
- **THEN** the turn output is returned normally
- **AND** the retry count is reset for the next turn

#### Scenario: Fourth retry attempt is rejected

- **WHEN** a turn has already been retried 3 times for `ContextOverflow`
- **AND** the 4th attempt also fails with `ContextOverflow`
- **THEN** the defect is converted to `ControlFlow::Break`
- **AND** an error "max defect retries (3) exceeded" is propagated
- **AND** no 5th retry is attempted

#### Scenario: Different defect types count toward the same limit

- **WHEN** a turn fails with `ContextOverflow` (retry 1), then `ToolExecutionFailure` (retry 2), then `ContextOverflow` again (retry 3)
- **THEN** the 4th attempt is rejected regardless of defect type
- **AND** the cumulative retry count is 3
