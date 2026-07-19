## ADDED Requirements

### Requirement: Replay reads JSONL events

The replay harness SHALL read a session's JSONL event log and
reconstruct the sequence of `TurnTask` records and the
`LoopContext`-equivalent state. Events with `ephemeral == true` SHALL be
skipped before entering the event-type match, ensuring replay only
processes durable (state-changing) events.

#### Scenario: Empty session replay
- **WHEN** the harness replays a session with no events
- **THEN** it returns an empty turn list and a default loop state

#### Scenario: Single turn replay
- **WHEN** the harness replays a session with one complete turn
- **THEN** it returns one `TurnTask` with status `Completed` and a
  loop state whose iteration count matches the event data

#### Scenario: Ephemeral events skipped during replay
- **WHEN** the harness replays a session containing both durable and
  ephemeral events (e.g., `LlmStreamDelta` with `ephemeral == true`)
- **THEN** ephemeral events are skipped without entering the match arms
  and only durable events affect the projected state

#### Scenario: Old-format JSONL without ephemeral field
- **WHEN** the harness replays a session written before the `ephemeral`
  field was added (all events default to `ephemeral == false`)
- **THEN** replay processes all events as durable, producing the same
  result as before the field was introduced
