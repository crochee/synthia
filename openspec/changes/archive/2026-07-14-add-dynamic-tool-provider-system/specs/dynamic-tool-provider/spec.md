# dynamic-tool-provider

## ADDED Requirements

### Requirement: ToolProvider trait SHALL define the extension point for dynamic tool registration

The `ToolProvider` trait provides a runtime-extensible interface for registering tools. Any implementor of `ToolProvider` can register tools without modifying the core agent code.

```rust
pub trait ToolProvider: Send + Sync {
    fn list_tools(&self) -> Vec<Arc<dyn Tool>>;
    fn on_event(&self, _event: &AgentEvent) -> Option<Vec<AgentEvent>> { None }
    fn before_tool_execute(&self, _tool: &str, _input: &Value) -> Option<ToolPreCheck> { None }
    fn after_tool_execute(&self, _tool: &str, _output: &Value) -> Option<Value> { None }
}
```

#### Scenario: Register custom tool at runtime
- **WHEN** A user calls `extension_manager.register(my_provider)` where `my_provider.list_tools()` returns a tool named "custom-tool"
- **THEN** The agent SHALL be able to call "custom-tool" in subsequent LLM calls

#### Scenario: Multiple providers register same tool name
- **WHEN** Provider A registers tool "read" and Provider B also registers tool "read"
- **THEN** The last-registered provider's tool SHALL win, and a warning SHALL be logged

### Requirement: ExtensionManager SHALL manage tool provider registration and caching

The `ExtensionManager` provides thread-safe registration and O(1) tool lookup via versioned cache invalidation.

#### Scenario: Register provider
- **WHEN** `extension_manager.register(provider)` is called
- **THEN** All tools from `provider.list_tools()` SHALL be immediately available via `list_tools()`

#### Scenario: Cache invalidation on registration
- **WHEN** A new provider is registered
- **THEN** The internal cache version counter SHALL be incremented and the tool cache SHALL be cleared

### Requirement: ToolProvider SHALL support lifecycle events

Providers MAY receive lifecycle events to synchronize state or perform cleanup.

#### Scenario: Provider receives tool execution event
- **WHEN** A tool managed by a provider is executed
- **THEN** The provider's `on_event()` SHALL be called with the `ToolCallCompleted` event if implemented

#### Scenario: Provider receives agent shutdown event
- **WHEN** The agent shuts down
- **THEN** Each provider's `on_shutdown()` SHALL be called if implemented
