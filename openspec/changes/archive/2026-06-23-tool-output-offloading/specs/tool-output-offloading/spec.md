## ADDED Requirements

### Requirement: Threshold-based offloading

The system SHALL compare each tool output against `MAX_BYTES=50KB` and `MAX_LINES=2000`; if either threshold is exceeded, the full output SHALL be written to the tool-output store and only a summary SHALL be returned to the LLM context.

#### Scenario: Output below threshold
- **WHEN** a tool returns 40KB and 1500 lines
- **THEN** the output is kept in context without offloading

#### Scenario: Output exceeds byte threshold
- **WHEN** a tool returns 60KB
- **THEN** the full output is written to the store and the context receives only a summary with the store path

#### Scenario: Output exceeds line threshold
- **WHEN** a tool returns 2500 lines
- **THEN** the full output is written to the store and the context receives only a summary with the store path

---

### Requirement: Summary format

The summary returned to the LLM context SHALL preserve the configured number of leading and trailing lines of the original output and SHALL include a marker indicating the full output path.

#### Scenario: Large output is offloaded
- **WHEN** a 10000-line output is offloaded to `/home/user/.synthia/tool-output/sess-abc/call-123.txt` with `head_lines=100` and `tail_lines=100`
- **THEN** the context contains the first 100 lines, the marker `[... {bytes} bytes / {lines} lines truncated; full output at /home/user/.synthia/tool-output/sess-abc/call-123.txt ...]`, and the last 100 lines

---

### Requirement: Store path and permissions

The tool-output store SHALL reside at `~/.synthia/tool-output/<session-id>/<tool-call-id>.txt`, parent directories SHALL be created on demand, and each file SHALL be created with permissions `0o600`.

#### Scenario: First offload in a session
- **WHEN** the first large output of a session is offloaded
- **THEN** the directory `~/.synthia/tool-output/<session-id>/` is created and the file is written with `0o600` permissions

---

### Requirement: Unified truncation entry point

All truncation and offloading decisions SHALL flow through `synthia_context::truncate::truncate_output`; no other module SHALL independently decide whether to offload a tool output.

#### Scenario: Tool orchestrator receives output
- **WHEN** the tool orchestrator receives a tool result
- **THEN** it delegates truncation/offloading to `truncate_output` and does not implement its own threshold checks

---

### Requirement: Asynchronous cleanup

The system SHALL delete tool-output files older than 7 days asynchronously at session startup and after each write.

#### Scenario: Session starts with stale files
- **WHEN** a session starts and finds files older than 7 days in `~/.synthia/tool-output/`
- **THEN** those files are deleted without blocking the session loop

#### Scenario: New offload is written
- **WHEN** a new offload file is persisted
- **THEN** an asynchronous cleanup pass is triggered for files older than 7 days

---

### Requirement: Access via existing read tool

The offloaded file path SHALL be exposed in the summary such that the model MAY retrieve the full output using the existing `read` tool without requiring a new tool definition.

#### Scenario: Model requests full output
- **WHEN** the model receives a summary containing `[... truncated; full output at /path ...]`
- **THEN** the model can call `read /path` to retrieve the complete content
