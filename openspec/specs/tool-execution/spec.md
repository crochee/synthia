## Purpose

Wrap every tool call with category-specific timeouts, end-to-end `CancellationToken` propagation, output truncation (head + tail), and an idempotent-only retry layer. This eliminates the "infinite-blocking slow command" failure mode and ensures tool execution is bounded, observable, and cancellable from any layer.
## Requirements
### Requirement: Tool execution SHALL enforce configurable timeouts

Each tool category SHALL have a default timeout. Tool execution SHALL be wrapped in tokio::time::timeout with the configured value. When timeout is exceeded, the tool call SHALL be cancelled and return a timeout error. The `subagent` tool category SHALL have a dedicated timeout of 300 seconds by default.

#### Scenario: Subagent task times out
- **WHEN** a foreground `task` tool call runs longer than the configured subagent timeout
- **THEN** the tool call SHALL be cancelled and return a timeout error

#### Scenario: Shell command times out
- **WHEN** a bash command runs longer than 60 seconds (default shell timeout)
- **THEN** the tool call SHALL be cancelled and return a timeout error

---

### Requirement: Tool execution results SHALL be truncated when exceeding size limit
When tool output exceeds 16KB, the result SHALL be truncated to retain the first 2KB and last 2KB, with the middle replaced by `[... truncated {N} bytes ...]`. The truncation event SHALL be recorded in the event log.

#### Scenario: Large file output is truncated
- **WHEN** a tool returns 100KB of output
- **THEN** the result SHALL contain the first 2KB, a truncation marker, and the last 2KB

### Requirement: Tool execution SHALL support CancellationToken-based cancellation
Each tool execution SHALL accept a CancellationToken. When the token is cancelled (via user interrupt or abort), the tool call SHALL be immediately cancelled and any partial results SHALL be discarded.

#### Scenario: User aborts during tool execution
- **WHEN** the user triggers an abort while a tool is executing
- **THEN** the tool call SHALL be cancelled immediately via CancellationToken

### Requirement: Idempotent tools SHALL support retry on failure
Tools marked as idempotent (read, search, fetch) SHALL retry up to 2 times with exponential backoff (1s, 3s) on timeout or temporary errors. The timeout for each retry attempt SHALL be reduced to leave time for remaining attempts.

#### Scenario: Network fetch retries on timeout
- **WHEN** a web_fetch tool times out on the first attempt
- **THEN** the system SHALL retry up to 2 times with 1s and 3s delays

### Requirement: Non-idempotent tools SHALL NOT be retried
Write operations (write, delete) and LLM calls SHALL NOT be automatically retried on failure to prevent data corruption or excessive cost.

#### Scenario: Write operation is not retried
- **WHEN** a write_file tool fails
- **THEN** the system SHALL NOT retry and SHALL return the error directly

### Requirement: Shell timeout SHALL have a configurable maximum上限
The maximum shell timeout SHALL be capped at 600 seconds regardless of any user-specified value. The default shell timeout SHALL be 60 seconds (unified from existing 120s default).

#### Scenario: Requested timeout is capped
- **WHEN** a tool requests a shell timeout of 1200 seconds
- **THEN** the actual timeout applied SHALL be 600 seconds (the maximum)

### Requirement: `build_default_tool_registry` SHALL accept optional subagent dependencies

The `build_default_tool_registry` function SHALL accept optional `AgentControl` and `SubagentSessionFactory` parameters. When both are provided, it SHALL register the `task` tool; when either is missing, it SHALL omit the `task` tool.

#### Scenario: Full subagent infrastructure available
- **WHEN** `build_default_tool_registry` is called with both `AgentControl` and `SubagentSessionFactory`
- **THEN** the returned registry SHALL contain the `task` tool

#### Scenario: Subagent infrastructure unavailable
- **WHEN** `build_default_tool_registry` is called without `AgentControl` or `SubagentSessionFactory`
- **THEN** the returned registry SHALL NOT contain the `task` tool

---

### Requirement: Tool registry construction SHALL remain backward compatible for callers without subagent infrastructure

Existing callers of `build_default_tool_registry` that pass only the workspace root SHALL continue to receive a registry with the basic tool set and without the `task` tool.

#### Scenario: Legacy caller uses old signature
- **WHEN** an existing call site invokes `build_default_tool_registry(workspace_root)`
- **THEN** the call SHALL compile and the returned registry SHALL NOT contain the `task` tool

