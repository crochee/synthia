## ADDED Requirements

### Requirement: CompactionAnalyticsAttempt SHALL track 5 fields per compaction

Every compaction attempt MUST be recorded as a `CompactionAnalyticsAttempt` struct with the following 5 fields: `active_context_tokens_before` (token count before compaction), `trigger` (what initiated the compaction: auto-threshold / manual / tool-call), `reason` (human-readable reason: "context-overflow-detected" / "max-iterations-reached" / etc.), `implementation` (which strategy ran: "stage1-soft-trim" / "stage2-hard-clear" / "stage3-pruning" / "anchored-summary"), `phase` (which phase of the strategy: "head-tail" / "replace" / "compress").

#### Scenario: Auto-triggered compaction records full telemetry

- **WHEN** context reaches 80% threshold and triggers Stage 1 Soft Trim
- **THEN** a `CompactionAnalyticsAttempt` is recorded with:
  - `active_context_tokens_before = 102400`
  - `trigger = "auto-threshold"`
  - `reason = "context-usage-80-percent"`
  - `implementation = "stage1-soft-trim"`
  - `phase = "head-tail"`
- **AND** the record is emitted before the next API call

#### Scenario: Tool-triggered compaction records different trigger

- **WHEN** the LLM calls `compact_context` tool explicitly
- **THEN** the `trigger` field is recorded as `"tool-call"`
- **AND** the `reason` field reflects the LLM's stated reason (or "llm-requested" if none provided)

---

### Requirement: CompactionAnalyticsAttempt SHALL be emitted as OTel span attributes

When the `otel` cargo feature is enabled, each `CompactionAnalyticsAttempt` MUST be emitted as OTel span attributes on the compaction span. The attribute keys MUST be: `compaction.active_context_tokens_before`, `compaction.trigger`, `compaction.reason`, `compaction.implementation`, `compaction.phase`. When `otel` is disabled, the record MUST be logged at `info` level instead.

#### Scenario: OTel-enabled emission

- **WHEN** `otel` feature is enabled and a compaction attempt occurs
- **THEN** the active span receives 5 attributes with the prefixed keys
- **AND** the attributes are visible in OTLP-collected traces

#### Scenario: OTel-disabled fallback to logging

- **WHEN** `otel` feature is disabled and a compaction attempt occurs
- **THEN** an `info!` log entry is emitted with all 5 fields
- **AND** no OTel span attributes are set (the span may not exist)

#### Scenario: Stage escalation records each stage separately

- **WHEN** a compaction escalates from Stage 1 to Stage 2 to Stage 3
- **THEN** three separate `CompactionAnalyticsAttempt` records are created
- **AND** each record has a different `implementation` field
- **AND** all three share the same `trigger` and `reason`
