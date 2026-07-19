# tool-output-truncate Specification

## Purpose
Automatic truncation of oversized tool outputs to prevent token waste and context pollution.

## ADDED Requirements

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
