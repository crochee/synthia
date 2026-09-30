# provider-hooks

## ADDED Requirements

### Requirement: Provider hooks SHALL execute before and after tool execution

The `before_tool_execute` and `after_tool_execute` hooks on `ToolProvider` SHALL be called at the appropriate points in the tool execution lifecycle.

```rust
pub enum ToolPreCheck {
    Allow,
    RequiresApproval(String),
    Deny(String),
}
```

#### Scenario: Before hook allows execution
- **WHEN** `provider.before_tool_execute("read", &input)` returns `Some(ToolPreCheck::Allow)`
- **THEN** Execution SHALL proceed immediately without calling `ApprovalService`

#### Scenario: Before hook requires approval
- **WHEN** `provider.before_tool_execute("bash", &input)` returns `Some(ToolPreCheck::RequiresApproval("shell command"))`
- **THEN** `ApprovalService::request_approval()` SHALL be called with the reason

#### Scenario: Before hook denies execution
- **WHEN** `provider.before_tool_execute("delete", &input)` returns `Some(ToolPreCheck::Deny("dangerous operation"))`
- **THEN** The tool SHALL NOT execute and an error SHALL be returned immediately

### Requirement: After hook MAY modify tool output

The `after_tool_execute` hook MAY return a modified output that replaces the original.

#### Scenario: After hook returns None
- **WHEN** `provider.after_tool_execute("read", &output)` returns `None`
- **THEN** The original output SHALL be used unchanged

#### Scenario: After hook returns modified output
- **WHEN** `provider.after_tool_execute("read", &output)` returns `Some(modified)`
- **THEN** The `modified` value SHALL be returned to the LLM

### Requirement: Hook errors SHALL NOT crash the runtime

If a hook itself errors, the runtime SHALL log the error and continue with default behavior.

#### Scenario: Before hook panics
- **WHEN** `provider.before_tool_execute(...)` panics
- **THEN** The panic SHALL be caught, logged as error, and execution SHALL proceed as if `Allow` was returned

#### Scenario: After hook returns error
- **WHEN** `provider.after_tool_execute(...)` returns `Err(e)`
- **THEN** The error SHALL be logged and the original output SHALL be returned
