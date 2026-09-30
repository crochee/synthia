## ADDED Requirements

### Requirement: ToolOrchestrator SHALL be the single entry point for all tool invocations
The system SHALL route every tool invocation, including built-in tools, MCP tools, and sub-agent tools, through a single `ToolOrchestrator` instance.

#### Scenario: Built-in tool invocation
- **WHEN** the agent invokes a built-in tool such as `bash` or `read_file`
- **THEN** the request SHALL pass through `ToolOrchestrator::execute` instead of directly calling the tool implementation.

#### Scenario: MCP tool invocation
- **WHEN** an MCP server exposes a tool and the model chooses to call it
- **THEN** the call SHALL be routed through `ToolOrchestrator::execute` with the same lifecycle as built-in tools.

---

### Requirement: ToolOrchestrator SHALL enforce approval policy before execution
The `ToolOrchestrator` SHALL query the configured `ApprovalService` when the effective permission for a tool invocation is `RequireConfirm` or `RequireExplicit`, and SHALL block execution until an explicit decision is received.

#### Scenario: Dangerous command requires confirmation
- **WHEN** the model invokes `bash` with a command classified as `RequireConfirm`
- **THEN** `ToolOrchestrator` SHALL call `ApprovalService::request_approval` and SHALL NOT spawn the process before approval is granted.

---

### Requirement: ToolOrchestrator SHALL select and apply a sandbox before execution
The `ToolOrchestrator` SHALL consult `SandboxManager` to select a sandbox profile based on the tool type and session policy, and SHALL wrap the execution command with the selected sandbox constraints.

#### Scenario: Bash command in Linux
- **WHEN** a `bash` tool runs on Linux with sandboxing enabled
- **THEN** `ToolOrchestrator` SHALL invoke `SandboxManager::select` and apply the returned `SandboxAttempt` to the command before execution.

---

### Requirement: ToolOrchestrator SHALL support cancellation of in-flight tool execution
The `ToolOrchestrator` SHALL accept a cancellation token and SHALL terminate the underlying process when cancellation is requested.

#### Scenario: User aborts long-running command
- **WHEN** a user sends an abort signal while a `bash` command is running
- **THEN** `ToolOrchestrator` SHALL cancel the execution and emit a cancelled result to the agent loop.

---

### Requirement: ToolOrchestrator SHALL aggregate results and emit structured events
After all tool calls in a turn complete, the `ToolOrchestrator` SHALL return a structured result set and emit events for execution start, completion, failure, and cancellation.

#### Scenario: Multiple parallel tools
- **WHEN** the model emits multiple tool calls in one turn
- **THEN** `ToolOrchestrator` SHALL execute them according to their concurrency policy and return a result entry for each call.

---

### Requirement: ToolOrchestrator SHALL provide a deterministic retry policy for transient failures
The `ToolOrchestrator` SHALL retry tool execution on transient failures using a configurable exponential backoff policy, up to a maximum retry count.

#### Scenario: Network tool times out
- **WHEN** an MCP network tool returns a transient timeout error
- **THEN** `ToolOrchestrator` SHALL retry the call up to the configured maximum retries before surfacing the error.
