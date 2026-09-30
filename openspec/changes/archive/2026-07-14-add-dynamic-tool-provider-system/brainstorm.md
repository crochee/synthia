# Brainstorm: add-dynamic-tool-provider-system

## Context

Synthia has a static `ToolRegistry` in `crates/synthia-tool/` that registers tools at compile time. Production-grade agents (opencode, codex, pi-mono) all have dynamic tool/extension systems that allow runtime registration.

**Deep research findings** (6 parallel directions, 27 source references):
- opencode: Effect Schema-driven tool definition, MCP tools dynamically discovered
- codex: Two-tier `ToolExecutor<Invocation>` → `CoreToolRuntime` hierarchy
- pi-mono: Extension trait with `on_ready()`/`on_shutdown()` lifecycle
- Synthia already has `DynamicResolver` in orchestrator layer (unutilized)
- Synthia already has parallel execution infrastructure (uncalled `execute_batch()`)

## Decision Chain

### Q1: Core goal - minimal runtime dynamic OR full extension system?

**Options:**
- A) Minimal: Just extend `ToolRegistry` to support `Arc<dyn ToolProvider>` registration
- B) Full: New `ToolProvider` trait as extension point, migrate all tools

**Decision: B (full extension system)**
- Minimal approach doesn't fully address the architectural gap
- Full system needed to match production-grade implementations
- Backward compatibility via adapter pattern

### Q2: Trait design - single flat trait OR layered hierarchy?

**Options:**
- A) Single `ToolProvider` trait with all methods
- B) Layered: `Tool` (base) → `ToolRuntime` (orchestration) → `DynToolProvider` (extension)

**Decision: B (layered, matching codex architecture)**
- Layered design separates concerns: tool definition vs execution orchestration vs dynamic registration
- codex proved this works at production scale
- Enables future plugin system as another provider layer

### Q3: Schema definition - manual JSON Schema OR schema library?

**Options:**
- A) Manual JSON Schema construction
- B) Use `schemars` + `serde_json_schema` derivation
- C) Effect Schema (like opencode) - but adds heavy dependency

**Decision: B (schemars derivation)**
- Synthia already uses `schemars` in some places
- Compile-time schema generation from Rust types
- No new heavy dependency

### Q4: Cache invalidation - version counter OR clear on registration?

**Options:**
- A) Version counter (O(1) on registration)
- B) Clear cache on every registration (O(n) but simple)

**Decision: A (version counter)**
- O(1) invalidation is important for frequent registrations
- Simple counter pattern proved in orchestrator layer

### Q5: Migration strategy - big bang OR incremental?

**Options:**
- A) Big bang: migrate all tools at once
- B) Incremental: adapter wrapper, migrate one tool at a time

**Decision: B (incremental)**
- Lower risk
- Can ship and test incrementally
- Existing code doesn't break during migration

## Design Trade-offs

### Approach 1: Codex-style Two-Tier (RECOMMENDED)

**Pros:**
- Production proven at scale (codex)
- Clear separation: tool definition vs runtime orchestration
- Enables parallel execution naturally

**Cons:**
- More complex initially
- Requires understanding two layers

### Approach 2: Single Flat Provider

**Pros:**
- Simpler to understand
- Lower initial complexity

**Cons:**
- Doesn't match production patterns
- Harder to extend later

### Approach 3: pi-mono Style Extension Trait

**Pros:**
- Lifecycle hooks built in
- Simple for extensions

**Cons:**
- Doesn't support parallel execution natively
- Less flexible than two-tier

## Selected Design: Two-Tier ToolProvider + ToolRuntime

```
┌─────────────────────────────────────────────────────────────┐
│  ExtensionManager                                          │
│  ├── register(Arc<dyn ToolProvider>)                      │
│  ├── list_tools() → HashMap<String, Arc<dyn Tool>>       │
│  └── invalidation_token: AtomicU64                        │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  ToolRuntime (orchestration layer)                         │
│  ├── execute_batch(requests) → parallel execution          │
│  ├── before_tool_execute hooks                           │
│  └── after_tool_execute hooks                            │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  Tool (base interface)                                    │
│  ├── name(), description()                                │
│  ├── parameters() → Schema                               │
│  ├── execute(input, ctx) → Future<ToolResult>            │
│  └── supports_parallel() → bool                           │
└─────────────────────────────────────────────────────────────┘
```

### Key Types

```rust
// New trait - extension point
pub trait ToolProvider: Send + Sync {
    fn list_tools(&self) -> Vec<Arc<dyn Tool>>;
    fn on_event(&self, _event: &AgentEvent) -> Option<Vec<AgentEvent>> { None }
    fn before_tool_execute(&self, _tool: &str, _input: &Value) -> Option<ToolPreCheck> { None }
    fn after_tool_execute(&self, _tool: &str, _output: &Value) -> Option<Value> { None }
}

// Existing Tool trait - kept compatible
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> &Schema;
    fn execute(&self, input: Value, ctx: ToolContext) -> impl Future<Output = Result<ToolResult>> + Send;
    fn supports_parallel(&self) -> bool { true }
}
```

## Migration Path

1. **Phase 1**: Add `ToolProvider` trait + `ExtensionManager`
2. **Phase 2**: Create adapter for existing static `ToolRegistry`
3. **Phase 3**: Migrate one tool category at a time to new system
4. **Phase 4**: Remove static registry, pure dynamic

## Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Breaking existing tools | Low | High | Adapter pattern, incremental migration |
| Performance regression | Medium | Medium | Benchmark existing vs new path |
| Complex lifetime management | Medium | Medium | RAII guards, clear ownership |

## Verification

- [ ] Unit tests for `ExtensionManager` registration
- [ ] Integration test: register tool at runtime, call it
- [ ] Benchmark: no regression vs static registration
- [ ] Migration test: existing tools work via adapter
