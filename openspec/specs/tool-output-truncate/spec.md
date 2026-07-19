# tool-output-truncate Specification

## Purpose
TBD - created by archiving change error-recovery-cascade. Update Purpose after archive.
## Requirements
### Requirement: Tool executor SHALL truncate outputs exceeding 16KB

When a tool returns output exceeding 16,384 bytes (16KB), the system SHALL truncate the output to include the first 8KB (head) and last 8KB (tail), separated by a truncation marker.

#### Scenario: Output exactly 16KB is not truncated
- **WHEN** a tool returns exactly 16,384 bytes of output
- **THEN** the output SHALL NOT be truncated

#### Scenario: Output exceeds 16KB triggers truncation
- **WHEN** a tool returns 50,000 bytes of output
- **THEN** the output SHALL be truncated to head(8,192 bytes) + marker + tail(8,192 bytes)
- **AND** the total truncated output SHALL be approximately 16,384 bytes

#### Scenario: Truncation marker is clearly visible
- **WHEN** output is truncated
- **THEN** the truncation marker SHALL contain the original byte count
- **AND** the marker SHALL be formatted as: `[... output truncated: showed 16384 of {original_len} bytes ...]`

---

### Requirement: ToolOutput SHALL report truncation status

The `ToolOutput` struct SHALL include fields to indicate whether truncation occurred and the original output size.

#### Scenario: ToolOutput carries truncation metadata
- **WHEN** a tool output is truncated
- **THEN** `ToolOutput.truncated` SHALL be `true`
- **AND** `ToolOutput.original_len` SHALL equal the original byte count before truncation

#### Scenario: ToolOutput indicates non-truncated output
- **WHEN** a tool output is within the 16KB limit
- **THEN** `ToolOutput.truncated` SHALL be `false`
- **AND** `ToolOutput.original_len` SHALL equal the actual output length

### Requirement: Tool output truncation SHALL be safe for multi-byte UTF-8 input

All tool output truncation paths (including `bash_tool::execute_command`) MUST use a UTF-8 safe byte-boundary detection function when truncating. The truncation MUST NOT call `String::truncate(usize)` directly on multi-byte UTF-8 input because that can panic with `byte index N is not a char boundary`.

#### Scenario: bash_tool truncation does not panic on multi-byte UTF-8
- **WHEN** `bash_tool::execute_command` returns output ending in a multi-byte UTF-8 character (e.g., Chinese, emoji) and the truncation point falls inside that character
- **THEN** the system SHALL NOT panic
- **AND** the truncated output SHALL end at a valid UTF-8 character boundary at or before the requested byte index

#### Scenario: Truncation marker remains after safe truncation
- **WHEN** bash_tool truncates a multi-byte UTF-8 string
- **THEN** the marker `"\n\n[stdout truncated at {max_output_length} bytes]"` SHALL still be appended after the safely-truncated content
- **AND** the appended content MUST be valid UTF-8

#### Scenario: cap_to_char_boundary helper is unit-tested
- **WHEN** the `cap_to_char_boundary` function is defined in `bash_tool.rs`
- **THEN** it SHALL be covered by unit tests with multi-byte UTF-8 inputs including Chinese characters, 4-byte emoji, and mixed content
- **AND** all tests SHALL verify the result is valid UTF-8 (re-parseable as `String`)

### Requirement: Cleared-placeholder rendering SHALL support both legacy and Anthropic tool-result shapes

The `truncate_messages` cleared-placeholder branch MUST surface the placeholder for tool-result messages regardless of which on-the-wire shape they use. The two supported shapes are:
1. **Anthropic / OpenAI shape**: `Role::User` (or `Role::Tool` with `tool_use_id` set) + `content = Content::Single(ContentPart::ToolResult(_))` — text lives inside `ToolResult.content[]` as `ContentPart::Text`.
2. **Legacy shape**: `Role::Tool` + `content = Content::Multi(vec![ContentPart::Text(_)])` + `tool_call_id = Some(_)` — text lives at the top level.

For shape (1), the renderer MUST replace the first `ContentPart::Text.text` inside the `ToolResult.content[]` array. For shape (2), the renderer MUST replace the top-level `ContentPart::Text.text`. For any other content variant (e.g., `ContentPart::Image(_)`), the renderer MUST treat the message as a no-op (no panic, no fallthrough).

#### Scenario: Shape 1 (Anthropic) cleared message renders placeholder
- **WHEN** a message has `content = Content::Single(ContentPart::ToolResult(_))` and `tool_result_cleared_at = Some(_)`
- **THEN** the first text inside `ToolResult.content[]` MUST be replaced with the cleared placeholder
- **AND** the `role`, `tool_use_id`, `is_error`, and `structured_content` fields MUST be preserved in storage

#### Scenario: Shape 2 (legacy) cleared message renders placeholder
- **WHEN** a message has `content = Content::Multi(vec![ContentPart::Text(_)])` and `tool_call_id = Some(_)` and `tool_result_cleared_at = Some(_)`
- **THEN** the top-level `ContentPart::Text.text` MUST be replaced with the cleared placeholder
- **AND** the `role` and `tool_call_id` fields MUST be preserved in storage

#### Scenario: Image content cleared message is a no-op
- **WHEN** a message has `content = Content::Single(ContentPart::Image(_))` and `tool_result_cleared_at = Some(_)`
- **THEN** the renderer MUST NOT panic
- **AND** the renderer MUST NOT modify the message
- **AND** the renderer MUST NOT fall through to size-based truncation

#### Scenario: Mixed content cleared message replaces only the first text-like field
- **WHEN** a message has `content = Content::Multi(vec![ContentPart::Text(_), ContentPart::ToolResult(_), ContentPart::Image(_)])` and `tool_result_cleared_at = Some(_)`
- **THEN** the first `ContentPart::Text` in the array MUST be replaced with the cleared placeholder
- **AND** the subsequent `ContentPart::ToolResult` and `ContentPart::Image` MUST NOT be modified

#### Scenario: Idempotent re-render produces identical output
- **WHEN** `truncate_messages` is called twice on the same set of cleared messages with the same `cfg`
- **THEN** the LLM-visible text after the second call MUST be byte-identical to the text after the first call
- **AND** the placeholder MUST NOT be re-stamped with a new timestamp

