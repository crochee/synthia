# tool-output-truncation Specification

## Purpose

Define the contract for `synthia_context::truncate::truncate_output`, a unified truncation service that preserves the head and tail of large tool outputs while spilling the full content to disk for later retrieval. This service is the single LLM-context-time truncation point; it coexists with the existing `tool_executor::truncate_result` for one release cycle.

## ADDED Requirements

### Requirement: TruncateConfig SHALL define truncation parameters

`synthia_context::truncate::TruncateConfig` SHALL expose `max_bytes: usize` (default 30_000), `head_lines: usize` (default 100), `tail_lines: usize` (default 100), and `temp_dir: PathBuf` (default `std::env::temp_dir().join("synthia-truncate")`).

#### Scenario: Default values applied
- **WHEN** `TruncateConfig::default()` is called
- **THEN** `max_bytes` SHALL be `30_000`
- **THEN** `head_lines` SHALL be `100`
- **THEN** `tail_lines` SHALL be `100`
- **THEN** `temp_dir` SHALL equal `std::env::temp_dir().join("synthia-truncate")`

#### Scenario: Custom values override defaults
- **WHEN** a caller constructs `TruncateConfig { max_bytes: 50_000, head_lines: 200, tail_lines: 50, temp_dir: ... }`
- **THEN** the custom values SHALL be used by `truncate_output` for that invocation

### Requirement: truncate_output SHALL preserve head and tail

`truncate_output(content: &str, cfg: &TruncateConfig) -> TruncatedResult` SHALL, when `content.len() > cfg.max_bytes`, return a result whose `output` is composed of the first `head_lines` lines, a marker line, and the last `tail_lines` lines. The full content SHALL be written to `cfg.temp_dir` and referenced by the marker.

#### Scenario: Small input is not truncated
- **WHEN** `content.len() <= cfg.max_bytes`
- **THEN** `TruncatedResult.truncated` SHALL be `false`
- **THEN** `TruncatedResult.output` SHALL equal `content`
- **THEN** `TruncatedResult.output_path` SHALL be `None`
- **THEN** no file SHALL be written to `cfg.temp_dir`

#### Scenario: Large input is truncated
- **WHEN** `content.len() > cfg.max_bytes` and `content` has more than `head_lines + tail_lines` lines
- **THEN** `TruncatedResult.truncated` SHALL be `true`
- **THEN** `TruncatedResult.output` SHALL start with the first `head_lines` of `content`
- **THEN** `TruncatedResult.output` SHALL end with the last `tail_lines` of `content`
- **THEN** `TruncatedResult.output` SHALL contain a marker of the form `"[... N bytes / M lines truncated; full output at <path> ...]"`
- **THEN** `TruncatedResult.output_path` SHALL be `Some(<written file path>)`
- **THEN** the file at `output_path` SHALL contain the full `content` byte-identical

#### Scenario: Disk write failure degrades gracefully
- **WHEN** the file write to `cfg.temp_dir` fails (e.g., permission denied, disk full)
- **THEN** `truncate_output` SHALL log a warning
- **THEN** it SHALL return a `TruncatedResult` with `output_path: None` and an in-memory truncated `output` (head + tail only, no marker path)
- **THEN** it SHALL NOT panic

#### Scenario: Empty input
- **WHEN** `content == ""` and any `cfg` is provided
- **THEN** `TruncatedResult.truncated` SHALL be `false`
- **THEN** `TruncatedResult.output` SHALL equal `""`
- **THEN** `TruncatedResult.output_bytes` SHALL be `0`

### Requirement: TruncatedResult SHALL be backward compatible with tool_executor

`TruncatedResult` fields SHALL include `#[serde(alias = "...")]` attributes for the legacy `tool_executor::truncate_result` field names: `content` ↔ `output`, `original_length` ↔ `original_bytes`, `truncated_length` ↔ `output_bytes`.

#### Scenario: Legacy field name deserializes
- **WHEN** a JSON payload uses the legacy key `content`
- **THEN** `serde_json::from_str` into `TruncatedResult` SHALL succeed and populate the `output` field
- **WHEN** a JSON payload uses the legacy key `original_length`
- **THEN** deserialization SHALL populate the `original_bytes` field

### Requirement: truncate_messages SHALL apply truncation to selected message roles

`truncate_messages(messages: &mut [ChatMessage], cfg: &TruncateConfig, role_predicate: impl Fn(&ChatMessage) -> bool) -> Vec<TruncatedResult>` SHALL apply `truncate_output` to any message for which `role_predicate` returns `true`, and return a `TruncatedResult` per affected message.

#### Scenario: Tool messages truncated
- **WHEN** `role_predicate` returns `true` for messages with role `Tool`
- **THEN** each Tool message whose content exceeds `cfg.max_bytes` SHALL be replaced with its truncated `output` in the slice
- **THEN** the function SHALL return one `TruncatedResult` per truncated message

#### Scenario: System messages untouched
- **WHEN** `role_predicate` returns `false` for messages with role `System`
- **THEN** System messages SHALL NOT be modified regardless of their content length

### Requirement: Truncation SHALL NOT change message order or role

Applying `truncate_messages` SHALL NOT change the order of messages, the role of any message, or the count of messages. It SHALL ONLY change the `content` field of affected messages.

#### Scenario: Order preserved
- **WHEN** `truncate_messages` is called on a slice of 5 messages where 2 are truncated
- **THEN** the resulting slice SHALL still contain exactly 5 messages in the same order
- **THEN** each message's `role` SHALL be unchanged

#### Scenario: Prefix cache compatibility
- **WHEN** the message slice is part of a TwoPartPrompt whose header_hash is computed from message order and role
- **THEN** `header_hash` SHALL remain stable across truncations (truncation is body-level, not header-level)
