# bash-utf8-safe-truncate Specification

## Purpose

The `bash_tool` execution path in `synthia-exec` must never panic on multi-byte UTF-8 character boundaries when truncating command output. This capability defines the safe truncation contract for `bash_tool::execute_command` and any other tool that performs byte-level `String::truncate` on command output.

## ADDED Requirements

### Requirement: bash_tool truncation SHALL NOT panic on multi-byte UTF-8 boundaries

The `bash_tool::execute_command` function MUST use a safe byte-boundary detection function (e.g., `cap_to_char_boundary`) when truncating stdout/stderr to `max_output_length`, instead of calling `String::truncate(usize)` directly. The safe function MUST scan backward from the requested byte index to the nearest valid UTF-8 character boundary.

#### Scenario: Truncation at multi-byte character boundary is safe
- **WHEN** a bash command produces output ending in a multi-byte UTF-8 character (e.g., Chinese, emoji) and the requested truncation point falls inside that character
- **THEN** the system SHALL NOT panic
- **AND** the truncated output SHALL end at a valid UTF-8 character boundary that is at or before the requested byte index

#### Scenario: Truncation at ASCII boundary behaves identically
- **WHEN** a bash command produces output containing only ASCII characters and the requested truncation point is at a character boundary
- **THEN** the truncated output SHALL be identical to direct `String::truncate(n)` behavior

#### Scenario: Truncation marker is preserved
- **WHEN** truncation occurs
- **THEN** the appended marker `"\n\n[stdout truncated at {N} bytes]"` or `"\n\n[stderr truncated at {N} bytes]"` SHALL still be appended after the safe-truncated content
- **AND** the marker SHALL reference the original `max_output_length` value, not the actual truncated byte count

#### Scenario: Truncated flag is set correctly
- **WHEN** the original output length exceeds `max_output_length`
- **THEN** the returned `truncated` flag SHALL be `true`
- **AND** when the original output length is at or below `max_output_length`
- **THEN** the returned `truncated` flag SHALL be `false`

### Requirement: cap_to_char_boundary function MUST be unit-tested with multi-byte inputs

A `cap_to_char_boundary(s: &mut String, max_bytes: usize)` helper function MUST be defined as a private function in `bash_tool.rs`. The function MUST be covered by regression tests that include inputs with Chinese characters, emoji (4-byte UTF-8 sequences), and mixed multi-byte content.

#### Scenario: Chinese characters near boundary
- **WHEN** input is `"你好世界测试字符串"` (each Chinese char is 3 bytes) and `max_bytes` falls in the middle of a character
- **THEN** the truncated string MUST end at the boundary of the previous character
- **AND** the result MUST be valid UTF-8 (re-parseable as `String`)

#### Scenario: Emoji (4-byte) near boundary
- **WHEN** input contains a 4-byte emoji character and `max_bytes` falls inside that emoji
- **THEN** the truncated string MUST exclude the incomplete emoji
- **AND** the result MUST be valid UTF-8

#### Scenario: Pure ASCII (no boundary adjustment)
- **WHEN** input is pure ASCII and `max_bytes` is at a character boundary
- **THEN** the result SHALL be identical to `s.truncate(max_bytes)`

#### Scenario: max_bytes larger than input
- **WHEN** `max_bytes` exceeds `s.len()`
- **THEN** the function MUST be a no-op (string unchanged)
- **AND** the result MUST be valid UTF-8

#### Scenario: max_bytes equals zero
- **WHEN** `max_bytes = 0`
- **THEN** the result MUST be an empty string
- **AND** MUST NOT panic

#### Scenario: Empty input
- **WHEN** input is an empty string and `max_bytes = N`
- **THEN** the result MUST be an empty string
- **AND** MUST NOT panic
