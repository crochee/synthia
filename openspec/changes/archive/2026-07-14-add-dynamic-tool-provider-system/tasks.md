## 1. Foundation: ToolProvider Trait and ExtensionManager

- [x] 1.1 Define `ToolProvider` trait in `crates/synthia-agent/src/tools/dynamic_provider.rs`
- [x] 1.2 Define `ToolPreCheck` enum (Allow/RequiresApproval/Deny)
- [x] 1.3 Define `SchemaRef` enum for schema references
- [x] 1.4 Create `ExtensionManager` struct with `Arc<RwLock<HashMap>>` cache
- [x] 1.5 Implement `ExtensionManager::register(provider)` with cache invalidation
- [x] 1.6 Implement `ExtensionManager::list_tools()` with O(1) lookup
- [x] 1.7 Add unit tests for `ExtensionManager` registration and cache invalidation

## 2. Tool Trait Alignment

- [x] 2.1 Review existing `ExecutableTool` trait in `synthia-tool`
- [x] 2.2 Align `Tool` trait interface with `ExecutableTool`
- [x] 2.3 Add `supports_parallel()` method to `Tool` trait (default: true)
- [x] 2.4 Ensure `ToolContext` and `ToolResult` types are compatible

## 3. ToolRuntime Orchestration Layer

- [x] 3.1 Create `ToolRuntime` struct in `crates/synthia-agent/src/tools/tool_runtime.rs`
- [x] 3.2 Wire `execute_batch()` call into `ToolRuntime::execute()`
- [x] 3.3 Integrate `before_tool_execute` hook calls per provider
- [x] 3.4 Integrate `after_tool_execute` hook calls per provider
- [x] 3.5 Add retry logic for `ToolError::Transient`
- [x] 3.6 Add unit tests for `ToolRuntime` with mock providers

## 4. StaticToolAdapter (Backward Compatibility)

- [x] 4.1 Create `StaticToolAdapter` wrapping `Arc<dyn ExecutableTool>`
- [x] 4.2 Implement `Tool` trait for `StaticToolAdapter`
- [x] 4.3 Ensure `supports_parallel()` delegates to inner tool
- [x] 4.4 Add integration test: wrap existing ReadFile tool, register, call

## 5. Agent Integration

- [x] 5.1 Add `extension_manager` field to `AgentRunConfig`
- [x] 5.2 Add `ExtensionManager::Builder` to `Agent::build()`
- [x] 5.3 Wire `ExtensionManager` into `StepToolExecute`
- [x] 5.4 Wire `ToolRuntime` into `StepToolExecute`
- [x] 5.5 Add integration test: register custom provider at runtime, agent uses its tools

## 6. Migration: Existing Tool Providers

- [x] 6.1 Create `FileToolsProvider` wrapping file tools (Read, Write, Edit, Glob, Grep)
- [x] 6.2 Create `BashToolsProvider` wrapping shell execution
- [x] 6.3 Create `SearchToolsProvider` wrapping search tools
- [x] 6.4 Create `MCPToolsProvider` for MCP dynamic tools
- [x] 6.5 Deprecate direct `register_defaults()` calls

## 7. Documentation and Examples

- [x] 7.1 Add doc comments to `ToolProvider` trait with example
- [x] 7.2 Write usage example in `crates/synthia-agent/examples/`
- [x] 7.3 Update `AGENTS.md` with dynamic tool registration guide
