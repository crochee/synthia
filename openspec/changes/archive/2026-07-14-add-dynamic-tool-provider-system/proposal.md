# Proposal: add-dynamic-tool-provider-system

## Why

Synthia's current `ToolRegistry` registers tools statically at compile time via `register_defaults()`. This prevents runtime tool registration, blocking third-party extensions, custom tools per-session, and dynamic capability discovery. Production agents (opencode, codex, pi-mono) all support dynamic tool registration as a core capability. Without this, Synthia cannot match the extensibility of production-grade agents.

## What Changes

**Tool Registration**
- From: Static `ToolRegistry::register()` only, called at startup
- To: Dynamic `ExtensionManager::register(Arc<dyn ToolProvider>)` at runtime
- Reason: Enable runtime registration without recompilation
- Impact: Non-breaking, additive API

**Tool Provider Model**
- From: All tools implement `ExecutableTool` directly
- To: Tools implement `Tool` (base), orchestrated by `ToolRuntime`, registered via `DynToolProvider`
- Reason: Match production-tier codex two-tier architecture
- Impact: Non-breaking for existing tools via adapter

**Capability Discovery**
- From: Tools discovered at compile time via `register_defaults()`
- To: `provider.list_tools()` returns tools dynamically
- Reason: Support MCP, plugins, per-session tools
- Impact: Non-breaking

## Capabilities

### New Capabilities

- `dynamic-tool-provider`: Core `ToolProvider` trait and `ExtensionManager` enabling runtime tool registration
- `tool-adapter`: Adapter wrapper allowing existing static tools to work via dynamic registration
- `tool-runtime`: Orchestration layer (`ToolRuntime`) handling parallel execution, hooks, and error recovery
- `provider-hooks`: `before_tool_execute` / `after_tool_execute` hooks per provider

### Modified Capabilities

- None (this is an additive change, existing tools work unchanged via adapter)

## Impact

**New files:**
- `crates/synthia-agent/src/tools/dynamic_provider.rs` - `ToolProvider` trait
- `crates/synthia-agent/src/tools/extension_manager.rs` - `ExtensionManager`
- `crates/synthia-agent/src/tools/tool_runtime.rs` - `ToolRuntime` orchestration layer
- `crates/synthia-agent/src/tools/adapter.rs` - `StaticToolAdapter` for existing tools

**Modified files:**
- `crates/synthia-agent/src/config/agent_config/run_config.rs` - Add `extension_manager` field
- `crates/synthia-agent/src/stream_builder/steps/tool_execute.rs` - Wire `ExtensionManager` into tool execution

**No breaking changes** to existing APIs or behaviors.
