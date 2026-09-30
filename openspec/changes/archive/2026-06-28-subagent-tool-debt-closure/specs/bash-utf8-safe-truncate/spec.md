<!--
Delta spec for modified capability: bash-utf8-safe-truncate
Adds requirement for output cap alignment with industry standard (1MB).
-->

## ADDED Requirements

### Requirement: bash_tool output cap SHALL be 1MB to align with industry standard

The `MAX_OUTPUT_BYTES` constant in `system_tools.rs` SHALL be set to `1_048_576` (1 MiB), aligning with opencode's `MAX_CAPTURE_BYTES = 1MB` and codex's `EXEC_OUTPUT_MAX_BYTES = 1MB`. The existing head+tail truncation logic with UTF-8 safe boundary checks (`find_safe_boundary`, `cap_to_char_boundary`) SHALL remain unchanged.

#### Scenario: Output under 1MB is not truncated
- **WHEN** a bash command produces 500KB of stdout
- **THEN** the full 500KB SHALL be returned to the LLM
- **AND** the `truncated` flag SHALL be `false`

#### Scenario: Output over 1MB is truncated with head+tail
- **WHEN** a bash command produces 2MB of stdout
- **THEN** the output SHALL be truncated to 1MB total using head+tail logic
- **AND** the `truncated` flag SHALL be `true`
- **AND** the truncation marker SHALL reference `1048576` bytes

#### Scenario: UTF-8 safety preserved at new cap
- **WHEN** a bash command produces output with multi-byte UTF-8 characters near the 1MB boundary
- **THEN** the truncation SHALL occur at a valid UTF-8 character boundary
- **AND** no panic SHALL occur

#### Scenario: Previous 30KB cap is superseded
- **WHEN** code references `MAX_OUTPUT_BYTES`
- **THEN** the value SHALL be `1_048_576`, not `30_000`
- **AND** any tests asserting 30KB behavior SHALL be updated
