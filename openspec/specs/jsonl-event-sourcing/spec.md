# jsonl-event-sourcing Specification

## Purpose
TBD - created by archiving change agent-event-ephemeral-classification. Update Purpose after archive.
## Requirements
### Requirement: Event payload schema

Each event SHALL include `seq`, `aggregate`, `event_type`, `ts`,
`source`, `ephemeral`, and a `payload` field; the payload MUST include
the current `turn_id` and `iteration` when emitted from the agent loop.
The `ephemeral` field SHALL be a boolean, defaulting to `false` (durable)
when absent during deserialization, ensuring backward compatibility with
existing JSONL files.

#### Scenario: Event structure with ephemeral field
- **WHEN** an event is read from the JSONL log
- **THEN** it deserializes into the enriched `PersistedEvent` struct
  and its payload contains `turn_id` and `iteration`, and the `ephemeral`
  field is present

#### Scenario: Backward compatibility with old JSONL
- **WHEN** an event line without the `ephemeral` field is deserialized
- **THEN** the `ephemeral` field defaults to `false` (durable) and
  replay processes the event as before

#### Scenario: Ephemeral event persisted with flag
- **WHEN** an ephemeral event is appended via `append_agent_event`
- **THEN** the persisted JSONL line contains `"ephemeral":true` and the
  returned `PersistedEvent` has `ephemeral == true`

