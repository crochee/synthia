## ADDED Requirements

### Requirement: Tool trait SHALL accept CancellationToken in call_with_sandbox

The `synthia_tool::Tool` trait's `call_with_sandbox()` method SHALL accept a `CancellationToken` parameter by reference. All built-in tools implementing this trait SHALL check the token at appropriate yield points and return `ToolError::Cancelled` when the token is canceled.

#### Scenario: Tool checks cancellation at entry
- **WHEN** a tool receives a `CancellationToken` that is already canceled at entry to `call_with_sandbox()`
- **THEN** the tool SHALL immediately return `Err(ToolError::Cancelled)`

#### Scenario: Tool yields and checks during chunked operation
- **WHEN** a tool processes data in chunks (e.g., large file writes, batch operations)
- **THEN** the tool SHALL call `tokio::task::yield_now().await` between chunks
- **AND** after each yield, check `token.is_cancelled()`
- **AND** if canceled, return `Err(ToolError::Cancelled)`

#### Scenario: Tool checks cancellation after blocking operation
- **WHEN** a tool performs a potentially long synchronous operation (e.g., regex compilation, parsing)
- **THEN** the tool SHALL check `token.is_cancelled()` after the operation completes
- **AND** if canceled during the operation, return `Err(ToolError::Cancelled)`

---

### Requirement: Tool trait SHALL accept CancellationToken in call_with_progress

The `synthia_tool::Tool` trait's `call_with_progress()` method SHALL accept a `CancellationToken` parameter and propagate it to the underlying `call_with_sandbox()`.

#### Scenario: Progress-based tool receives and propagates cancellation
- **WHEN** `call_with_progress()` is called with a `CancellationToken`
- **THEN** the token SHALL be passed to `call_with_sandbox()`
- **AND** cancellation SHALL propagate to the underlying tool execution

---

### Requirement: ToolAdapter SHALL propagate cancellation token

`ToolAdapter::execute()` SHALL pass the received `cancellation_token` parameter to `self.tool.call_with_sandbox()` instead of discarding it. The `_cancellation_token` parameter name (underscore prefix) SHALL be changed to `cancellation_token` (no underscore).

#### Scenario: ToolAdapter passes token to tool
- **WHEN** `ToolAdapter::execute()` is called with a `CancellationToken`
- **THEN** the token SHALL be passed to `self.tool.call_with_sandbox(input, sandbox_attempt, cancellation_token)`
- **AND** the token SHALL NOT be ignored or dropped

#### Scenario: ToolAdapter passes token via call_with_progress path
- **WHEN** `ToolAdapter::execute_with_events()` is called with a `CancellationToken`
- **THEN** the token SHALL be passed through the `call_with_progress()` path to the underlying tool

---

### Requirement: ToolRegistry registry path SHALL propagate cancellation

When executing tools via `ToolRegistry::run_with_context()`, the `cancel_token` SHALL be passed through the registry's execution context to the tool's `call_with_sandbox()`.

#### Scenario: Registry path receives and propagates cancellation
- **WHEN** `StepToolExecute` executes a tool via `execute_via_registry()`
- **THEN** the `cancel_token` SHALL be passed to the registry's execution context
- **AND** the registry SHALL propagate the token to the tool's `call_with_sandbox()`

---

### Requirement: Built-in tools SHALL add cooperative yield points

All built-in tools that perform chunked operations SHALL add `tokio::task::yield_now().await` at appropriate intervals with cancellation checks.

#### Scenario: Read tool yields during large file read
- **WHEN** `ReadTool::call_with_sandbox()` reads a file larger than 64KB
- **THEN** the read SHALL be chunked (e.g., 64KB chunks)
- **AND** between each chunk, `tokio::task::yield_now().await` SHALL be called
- **AND** cancellation SHALL be checked after each yield

#### Scenario: Write tool yields during large file write
- **WHEN** `WriteTool::call_with_sandbox()` writes a file larger than 64KB
- **THEN** the write SHALL be chunked (e.g., 64KB chunks)
- **AND** between each chunk, `tokio::task::yield_now().await` SHALL be called
- **AND** cancellation SHALL be checked after each yield

#### Scenario: Glob tool yields during large directory scan
- **WHEN** `GlobTool::call_with_sandbox()` scans a directory tree with many entries
- **THEN** after processing each directory level, `tokio::task::yield_now().await` SHALL be called
- **AND** cancellation SHALL be checked before proceeding to the next level

#### Scenario: Grep tool yields during large file scan
- **WHEN** `GrepTool::call_with_sandbox()` searches files matching a pattern
- **THEN** after each file is processed, `tokio::task::yield_now().await` SHALL be called
- **AND** cancellation SHALL be checked before reading the next file
