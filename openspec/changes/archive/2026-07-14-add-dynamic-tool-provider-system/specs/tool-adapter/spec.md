# tool-adapter

## ADDED Requirements

### Requirement: StaticToolAdapter SHALL wrap existing ExecutableTool for dynamic registration

The `StaticToolAdapter` provides backward compatibility by wrapping existing `ExecutableTool` implementations as `Arc<dyn Tool>`.

```rust
pub struct StaticToolAdapter {
    inner: Arc<dyn ExecutableTool>,
}

impl Tool for StaticToolAdapter {
    fn name(&self) -> &str { self.inner.name() }
    fn description(&self) -> &str { self.inner.description() }
    fn parameters(&self) -> &Schema { self.inner.parameters() }
    fn execute(&self, input: Value, ctx: ToolContext) -> impl Future<Output = Result<ToolResult>> + Send {
        self.inner.execute(input, ctx)
    }
    fn supports_parallel(&self) -> bool { true }
}
```

#### Scenario: Wrap ReadFile tool for dynamic registration
- **WHEN** `StaticToolAdapter::new(read_file_tool)` is called
- **THEN** The resulting `Arc<dyn Tool>` SHALL be registrable via `ExtensionManager`

### Requirement: Adapter SHALL preserve execution semantics

The adapter SHALL NOT modify the underlying tool's behavior, error handling, or execution guarantees.

#### Scenario: Tool returns error
- **WHEN** The wrapped tool returns `Err(ToolError::NotFound)`
- **THEN** The adapter SHALL propagate the same error to the caller

#### Scenario: Tool supports parallel execution
- **WHEN** The wrapped tool's `supports_parallel()` returns `true`
- **THEN** The adapter's `supports_parallel()` SHALL return `true`
