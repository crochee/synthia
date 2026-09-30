<!--
Delta spec for tool-output-truncate capability.
Modifies openspec/specs/tool-output-truncate/spec.md
-->

## ADDED Requirements

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
