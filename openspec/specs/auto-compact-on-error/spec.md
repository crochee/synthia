# auto-compact-on-error Specification

## Purpose
TBD - created by archiving change error-recovery-cascade. Update Purpose after archive.
## Requirements
### Requirement: L3 fallback failure with high context SHALL trigger auto-compact

When L3 (Fallback) escalation occurs AND the context token ratio exceeds 80%, the system SHALL attempt to compact the conversation before escalating to L5.

#### Scenario: High context ratio triggers auto-compact
- **WHEN** L3 fallback fails AND `context.token_ratio() > 0.8`
- **THEN** the system SHALL call `compact_with_fallback()`
- **AND** the compaction SHALL attempt Level 1 (LLM summary) first

#### Scenario: Low context ratio skips auto-compact
- **WHEN** L3 fallback fails AND `context.token_ratio() <= 0.8`
- **THEN** the system SHALL NOT attempt auto-compact
- **AND** the failure SHALL escalate directly to L5 (Reset)

---

### Requirement: Auto-compact SHALL use the existing fallback chain

The compaction SHALL use the existing `compact_with_fallback()` implementation with L1 → L2 → L3 degradation.

#### Scenario: L1 (LLM summary) succeeds
- **WHEN** auto-compact is triggered and LLM summary fits within budget
- **THEN** the conversation SHALL be compacted using L1
- **AND** `record_success()` SHALL be called

#### Scenario: L1 exceeds budget falls to L2
- **WHEN** auto-compact L1 produces output exceeding budget
- **THEN** the system SHALL fall back to L2 (structured truncation)
- **AND** the compaction SHALL proceed with L2

#### Scenario: L2 exceeds budget falls to L3
- **WHEN** auto-compact L2 produces output exceeding budget
- **THEN** the system SHALL fall back to L3 (marker-only)
- **AND** the compaction SHALL proceed with L3

---

### Requirement: Compaction failure SHALL escalate to L5

When all compaction levels fail to reduce context within budget, the system SHALL escalate to L5 (Reset).

#### Scenario: All compaction levels exhausted
- **WHEN** L1, L2, and L3 compaction all fail to meet budget
- **THEN** the failure SHALL escalate to L5 (Reset)
- **AND** the consecutive error counter SHALL be incremented

#### Scenario: Compaction preserves critical information
- **WHEN** compaction is performed
- **THEN** the most recent messages SHALL be preserved intact
- **AND** older messages SHALL be candidates for compaction

