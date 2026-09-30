# tool-runtime

## ADDED Requirements

### Requirement: ToolRuntime SHALL orchestrate tool execution with hooks and error recovery

The `ToolRuntime` is the orchestration layer that handles parallel execution, before/after hooks, and error recovery uniformly.

```rust
pub struct ToolRuntime {
    resolver: Arc<dyn ToolResolver>,
    approval_service: Arc<dyn ApprovalService>,
    sandbox_manager: Arc<dyn SandboxManager>,
    retry_policy: RetryPolicy,
}
```

#### Scenario: Execute batch of tools in parallel
- **WHEN** `execute_batch(requests, &ctx)` is called with 3 independent tool calls
- **THEN** The tools SHALL be executed concurrently and all results SHALL be returned

#### Scenario: Tool fails with transient error
- **WHEN** A tool returns `ToolError::Transient`
- **THEN** The runtime SHALL retry according to `retry_policy` before propagating the error

### Requirement: ToolRuntime SHALL support before/after hooks per provider

Each `ToolProvider` MAY provide `before_tool_execute` and `after_tool_execute` hooks that run around tool execution.

#### Scenario: Provider approves tool execution
- **WHEN** Provider A's `before_tool_execute("bash", input)` returns `ToolPreCheck::Allow`
- **THEN** Tool execution SHALL proceed normally

#### Scenario: Provider blocks tool execution
- **WHEN** Provider A's `before_tool_execute("bash", input)` returns `ToolPreCheck::Deny("risky command")`
- **THEN** Tool execution SHALL be blocked and error SHALL be returned

#### Scenario: Provider modifies tool output
- **WHEN** Provider A's `after_tool_execute("read", output)` returns `Some(modified_output)`
- **THEN** The modified output SHALL be returned to the LLM instead of the original

### Requirement: ToolRuntime SHALL handle tool pre-check outcomes

The runtime SHALL interpret `ToolPreCheck` enum and take appropriate action.

#### Scenario: Approval required
- **WHEN** `before_tool_execute` returns `ToolPreCheck::RequiresApproval(reason)`
- **THEN** The runtime SHALL invoke `ApprovalService` and await approval before proceeding
