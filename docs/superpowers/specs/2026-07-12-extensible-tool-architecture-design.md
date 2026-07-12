# Extensible Tool-Based Agent Architecture Design

**Date**: 2026-07-12
**Status**: Proposed
**Author**: Sisyphus (via agent analysis)

---

## Context

Synthia currently has a well-structured Rust-based AI agent framework, but compared to production-grade implementations (opencode, codex, pi-mono), it has architectural gaps in extensibility, event sourcing, session management, and parallel tool execution.

This design proposes refactoring the non-core logic into pluggable tools and extensions while keeping the React loop minimal.

---

## Design Principles

1. **ToolProvider as Extension Point** - All functionality beyond core loop is a `ToolProvider`
2. **Schema-Driven** - Tools use typed schemas for validation
3. **Event-Driven** - Session state updates through events
4. **Zero-Cost Abstraction** - Static dispatch where possible, dynamic where needed

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     Agent Runtime                                │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐   │
│  │ React Loop  │  │   Tool     │  │    Extension        │   │
│  │ (minimal)   │  │ Orchestrator│  │    Manager         │   │
│  └─────────────┘  └─────────────┘  └─────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                 ToolProvider Trait (Extension Point)            │
│  fn list_tools() -> Vec<Arc<dyn Tool>>                        │
│  fn on_event(event: &Event) -> Option<Vec<AgentEvent>>        │
│  fn before_tool_execute(tool: &str, input: &Value) -> Opt     │
│  fn after_tool_execute(tool: &str, output: &Value) -> Opt     │
└─────────────────────────────────────────────────────────────────┘
```

---

## Core Traits

### ToolProvider (Extension Point)

```rust
pub trait ToolProvider: Send + Sync {
    fn list_tools(&self) -> Vec<Arc<dyn Tool>>;
    fn on_event(&self, _event: &AgentEvent) -> Option<Vec<AgentEvent>> { None }
    fn before_tool_execute(&self, _tool: &str, _input: &Value) -> Option<ToolPreCheck> { None }
    fn after_tool_execute(&self, _tool: &str, _output: &Value) -> Option<Value> { None }
}
```

### Tool (Built-in Functionality)

```rust
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> &Schema;
    fn execute(&self, input: Value, ctx: ToolContext) -> impl Future<Output = Result<ToolResult>> + Send;
    fn supports_parallel(&self) -> bool { true }
}
```

---

## ExtensionManager

```rust
pub struct ExtensionManager {
    providers: RwLock<Vec<Arc<dyn ToolProvider>>>,
    tool_cache: RwLock<HashMap<String, Arc<dyn Tool>>>,
}

impl ExtensionManager {
    pub fn register(&self, provider: Arc<dyn ToolProvider>) {
        // Invalidate cache, register tools
    }

    pub async fn reload(&self, extension_path: &Path) -> Result<()> {
        // Dynamic .so loading
    }
}
```

---

## Built-in Extensions

| Extension | Tools Provided | Priority |
|-----------|---------------|----------|
| FileToolsExtension | read, write, edit, glob, grep | High |
| BashToolsExtension | bash, shell | High |
| SessionExtension | fork, branch, navigate | High |
| GuardianExtension | doom_loop, self_reflect | Medium |
| CompactionExtension | compact_context | Medium |
| MCPExtension | mcp_* | Medium |
| SteeringExtension | steer, followUp | Low |

---

## Migration Path

### Phase 1: Extract ToolProviders

1. Create `GuardianExtension` wrapping `LoopDetectorSet`
2. Create `FileToolsExtension` wrapping file operations
3. Create `SessionExtension` for session management

### Phase 2: Add Parallel Execution

1. Modify `ToolOrchestrator` to support `supports_parallel()`
2. Use `tokio::join!` for parallel tool calls when all tools support it

### Phase 3: Dynamic Loading

1. Implement `ExtensionManager::reload()` with `libloading`
2. Add hot-reload configuration

---

## Events (Proposed)

| Event | Payload | Purpose |
|-------|---------|---------|
| `SessionStarted` | session_id | Session init |
| `TurnStarted` | turn_id, iteration | Turn begin |
| `ToolCallIssued` | tool, input | Tool called |
| `ToolResultReceived` | tool, output | Tool completed |
| `TurnCompleted` | turn_id | Turn end |
| `SessionEnded` | reason | Session done |

---

## Open Questions

1. Should ExtensionManager support unloading (not just reloading)?
2. How to handle extension priority/ordering?
3. Schema evolution strategy for tool parameters?

---

## References

- opencode: `/home/crochee/workspace/opencode/packages/core/src/tool/registry.ts`
- codex: `/home/crochee/workspace/codex/codex-rs/core/src/tools/registry.rs`
- pi-mono: `/home/crochee/workspace/pi-mono/packages/coding-agent/src/core/extensions/runner.ts`
