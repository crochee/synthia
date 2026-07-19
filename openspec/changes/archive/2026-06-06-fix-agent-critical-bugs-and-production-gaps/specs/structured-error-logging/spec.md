## ADDED Requirements

### Requirement: Critical path errors SHALL be logged with structured output

When a critical operation (hook execution, memory event sending, session directory creation) fails, the agent SHALL log the error using `tracing::warn!` or `tracing::error!` with the error message and context, rather than silently ignoring the failure via `let _ =`.

#### Scenario: Silent error is replaced with tracing::warn
- **WHEN** a critical operation fails (e.g., hook execution, memory event send)
- **THEN** the agent SHALL log the error with `tracing::warn!` or `tracing::error!` instead of silently ignoring it

### Requirement: Error logging SHALL preserve error context

For each silent error swallowing location, the error SHALL be logged with:
- The operation that failed (e.g., "before_llm hook", "memory event")
- The error message
- Any available context (session_id, iteration, etc.)

#### Scenario: before_llm hook failure is logged
- **WHEN** `fire_before_llm` returns an error
- **THEN** the agent SHALL log a warning with the error message
- **AND** continue execution without blocking the main flow

#### Scenario: after_llm hook failure is logged
- **WHEN** `fire_after_llm` returns an error
- **THEN** the agent SHALL log a warning with the error message
- **AND** continue execution without blocking the main flow

#### Scenario: Memory event send failure is logged
- **WHEN** sending a `MemoryEvent::tool_executed` fails
- **THEN** the agent SHALL log a warning with the event details
- **AND** continue execution without blocking the main flow

#### Scenario: Session directory creation failure is logged
- **WHEN** `ensure_session_dir` fails
- **THEN** the agent SHALL log a warning with the session_id
- **AND** continue execution without blocking the main flow

---

## MODIFIED Requirements

None — this is a new capability.