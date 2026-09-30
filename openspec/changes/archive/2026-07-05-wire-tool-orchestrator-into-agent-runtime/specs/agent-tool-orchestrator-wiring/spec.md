## ADDED Requirements

### Requirement: Agent runtime SHALL assemble a default ToolOrchestrator when none is explicitly provided

The `Agent::run_stream` and `Agent::resume` paths SHALL construct a `DefaultToolOrchestrator` using the configured `ApprovalService` and `SandboxManager` whenever `AgentRunConfig.tool_orchestrator` is `None` and at least one of those services is available. This ensures the production tool-execution path is active by default.

#### Scenario: Default orchestrator assembly on run_stream
- **WHEN** an `Agent` is started via `run_stream` without an explicit `tool_orchestrator`
- **AND** an `approval_service` and/or `sandbox_manager` has been provided (or defaulted)
- **THEN** `AgentRunConfig.tool_orchestrator` SHALL be set to a `DefaultToolOrchestrator` built by `build_default_tool_orchestrator()` before the stream begins.

#### Scenario: Default orchestrator assembly on resume
- **WHEN** an `Agent` resumes a session via `resume` without an explicit `tool_orchestrator`
- **AND** an `approval_service` and/or `sandbox_manager` has been provided (or defaulted)
- **THEN** `AgentRunConfig.tool_orchestrator` SHALL be set to a `DefaultToolOrchestrator` built by `build_default_tool_orchestrator()` before the stream begins.

#### Scenario: Explicit orchestrator is preserved
- **WHEN** a caller explicitly injects a `tool_orchestrator` into `AgentRunConfig`
- **THEN** `Agent::run_stream` and `Agent::resume` SHALL NOT replace it with the default orchestrator.

---

### Requirement: Agent SHALL expose injection points for ApprovalService and SandboxManager

The `Agent` struct SHALL provide optional `approval_service` and `sandbox_manager` fields, along with `with_approval_service()` and `with_sandbox_manager()` builder-style methods, so that CLI, server, and test callers can override the default services without modifying `AgentRunConfig` directly.

#### Scenario: CLI injects a TUI approval service
- **WHEN** the CLI constructs an `Agent` and calls `agent.with_approval_service(tui_approval_service)`
- **THEN** the provided service SHALL be used when assembling the default `ToolOrchestrator`.

#### Scenario: Server injects a sandbox manager
- **WHEN** the server constructs an `Agent` and calls `agent.with_sandbox_manager(sandbox_manager)`
- **THEN** the provided manager SHALL be used when assembling the default `ToolOrchestrator`.

---

### Requirement: The default ApprovalService SHALL be fail-closed

When no `ApprovalService` is injected, the agent SHALL default to `HeadlessApprovalService`, which MUST deny approval requests for tools that require confirmation. This preserves the project's fail-closed security policy.

#### Scenario: Headless mode denies bash tool
- **WHEN** an agent runs in headless mode without a custom `ApprovalService`
- **AND** the model issues a `bash` tool call that requires confirmation
- **THEN** the `DefaultToolOrchestrator` SHALL receive a denial from `HeadlessApprovalService` and the tool call SHALL fail with a denied error.

---

### Requirement: The default SandboxManager SHALL preserve existing sandbox behavior

When no `SandboxManager` is injected, the agent SHALL default to `NoopSandboxManager`, so that enabling the orchestrator does not unexpectedly change the current sandboxing behavior.

#### Scenario: Noop sandbox in default configuration
- **WHEN** an agent runs without a custom `SandboxManager`
- **THEN** `DefaultToolOrchestrator` SHALL use `NoopSandboxManager` and tools SHALL execute with the same sandbox semantics as before this change.

---

### Requirement: ToolRegistry fallback path SHALL remain available

`StepToolExecute` SHALL continue to support direct `ToolRegistry` execution when `AgentRunConfig.tool_orchestrator` is explicitly `None`. This preserves backward compatibility for callers that bypass the orchestrator.

#### Scenario: Explicit None falls back to registry
- **WHEN** a caller builds an `AgentRunConfig` with `tool_orchestrator: None`
- **THEN** `StepToolExecute` SHALL execute tool calls through `ToolRegistry::run_with_context`.

---

## MODIFIED Requirements

无现有 spec 的需求变更。

## REMOVED Requirements

无删除的需求。

## RENAMED Requirements

无重命名的需求。
