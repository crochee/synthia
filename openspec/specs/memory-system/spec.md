## Purpose

Augment the existing Phase 1/2 memory pipeline with a bypass append-only event log (JSONL, credential-redacted), a `memory_search` tool, and a context-injection strategy. This ensures no information is silently dropped and historical tool calls / decisions / errors remain retrievable on demand.

## Requirements

### Requirement: All tool events SHALL be logged to append-only JSONL event log
Every tool call, decision, error, file modification, and cron execution SHALL be recorded as an event in `~/.synthia/memories/events/YYYY-MM-DD.jsonl`. The event log SHALL be append-only — events SHALL NEVER be modified or deleted by the system. Events SHALL be desensitized through credential_guard before writing.

#### Scenario: Tool call is logged
- **WHEN** a tool is called (e.g., read_file)
- **THEN** an event with type "tool_call", tool name, input, and output SHALL be appended to the daily JSONL file

#### Scenario: Sensitive data is redacted before logging
- **WHEN** a tool output contains an API key or secret
- **THEN** the event SHALL have the sensitive value replaced with `[REDACTED]`

### Requirement: Large tool outputs SHALL be limited in event log
When tool output exceeds 10KB, the event log SHALL store only the first 10KB. The full output SHALL be stored in a separate file under `~/.synthia/memories/raw_outputs/` and referenced by hash in the event log.

#### Scenario: 1MB file read is logged with size limit
- **WHEN** read_file returns 1MB of content
- **THEN** the event log SHALL store only 10KB and reference the full content by hash

### Requirement: Agent SHALL provide memory_search tool
The memory_search tool SHALL search the event log using ripgrep for keyword matching. It SHALL accept query string, event type filter (all | decision | error | tool_call | file_modified), and result limit. Results SHALL be returned as tool_result and injected at the end of context.

#### Scenario: Search returns matching events
- **WHEN** agent calls memory_search with query "认证文件"
- **THEN** the system SHALL return events containing "认证文件" sorted by relevance

#### Scenario: Search results are desensitized
- **WHEN** memory_search returns events containing sensitive data
- **THEN** the results SHALL have sensitive values redacted through credential_guard

### Requirement: Memory SHALL be injected at defined trigger points
Memory injection SHALL occur at: session startup (load active_memories.md); when agent is asked about personal preferences (memory_search); during cron task execution (attach relevant history); after 10+ consecutive tool calls (read todo.md for P5 recency anchoring); at task switch (read todo.md to re-align).

#### Scenario: Active memories loaded on session start
- **WHEN** a new session is created
- **THEN** the system SHALL load `~/.synthia/memories/active_memories.md` into context

#### Scenario: Todo.md read after long tool sequence
- **WHEN** the agent has executed 10+ tools in a row without reading todo.md
- **THEN** the system SHALL prompt the agent to read todo.md for recency anchoring

### Requirement: Event log files SHALL have restricted file permissions
All event log files SHALL be created with permission `0600` (owner read/write only). The `~/.synthia/memories/` directory SHALL be created with permission `0700` (owner only).

#### Scenario: Event log file has correct permissions
- **WHEN** a new event log file is created
- **THEN** the file permissions SHALL be `0600`
