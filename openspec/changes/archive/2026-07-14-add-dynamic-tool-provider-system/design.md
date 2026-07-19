# Design: add-dynamic-tool-provider-system

## Context

Synthia has a static `ToolRegistry` in `crates/synthia-tool/` that registers tools at compile time via `register_defaults()` and manual `register()` calls. Production-grade agents (opencode, codex, pi-mono) all have dynamic tool/extension systems enabling runtime registration.

**Current state:**
- `ToolRegistry` in `crates/synthia-tool/` is static (compilation-time registration only)
- `DynamicResolver` exists in `crates/synthia-tool-orchestrator/` but unused in agent path
- `ToolOrchestrator::execute_batch()` exists but never called (sequential execution only)
- Two separate hook systems exist (`synthia-hook` and `synthia-plugin`) with overlapping concerns

**Key constraints:**
- Must maintain backward compatibility with existing tools
- No breaking changes to public APIs
- Rust code requiring C ABI compatibility for future plugin system

## Goals / Non-Goals

**Goals:**
- Enable runtime tool registration without recompilation
- Add `ToolProvider` trait as the extension point
- Layer architecture: `Tool` (base) → `ToolRuntime` (orchestration) → `DynToolProvider` (dynamic)
- Incremental migration path for existing static tools
- Enable future plugin system (extraction to `.so`)

**Non-Goals:**
- Not implementing plugin loading (that's a separate P4 change)
- Not changing the tool execution semantics (parallel via existing infrastructure)
- Not migrating all existing tools in this change (incremental per-provider)
- Not adding schema generation (use existing `schemars` patterns)

## Decisions

### D1: Trait architecture - two-tier vs single flat

**Chosen:** Two-tier: `Tool` (base) → `ToolRuntime` (orchestration) → `DynToolProvider` (extension)

**Rationale:** Matches codex's production-proven architecture. Separates tool definition from execution orchestration from dynamic registration. Enables `ToolRuntime` to handle parallel execution, hooks, and error recovery uniformly.

**Alternatives considered:**
- Single flat `ToolProvider` trait: Rejected - mixes concerns, harder to extend later
- pi-mono-style `Extension` trait: Rejected - lacks built-in parallel execution support

### D2: Schema definition - derive vs manual

**Chosen:** `schemars::JsonSchema` derive macro

**Rationale:** Synthia already uses `schemars` in some places. Compile-time schema from Rust types avoids drift between types and schemas. No new heavy dependencies.

**Alternatives considered:**
- Effect Schema (opencode): Rejected - heavy dependency, steep learning curve
- Manual JSON Schema construction: Rejected - error-prone, hard to maintain

### D3: Cache invalidation - version counter vs clear

**Chosen:** `AtomicU64` version counter + `DashMap`

**Rationale:** O(1) invalidation on registration. Existing `DynamicResolver` already uses this pattern.

**Alternatives considered:**
- Clear cache on registration: Rejected - O(n) on every registration
- RwLock on cache: Rejected - more complex, same performance

### D4: Migration strategy - big bang vs incremental

**Chosen:** Incremental with adapter pattern

**Rationale:** Lower risk, can ship and test per-tool-category. Adapter wrapper (`StaticToolAdapter`) lets existing tools work unchanged during migration.

**Alternatives considered:**
- Big bang migration: Rejected - high risk, all tools must migrate simultaneously

### D5: Hook integration - extend existing vs new

**Chosen:** Extend existing `HookRegistry` via `before_tool_execute` / `after_tool_execute` in `ToolProvider`

**Rationale:** Leverages existing `synthia-hook` infrastructure. `HookProvider` adapter wraps `HookRegistry` as a `ToolProvider`.

**Alternatives considered:**
- Separate hook system for providers: Rejected - duplicate mechanism
- Inline hooks in each tool: Rejected - no sharing, inconsistent

## Risks / Trade-offs

[Performance] Dynamic dispatch adds ~5-10% overhead per tool call vs static dispatch → Mitigation: Cache hot paths, use `Arc` clones sparingly

[Complexity] Two-tier trait hierarchy increases cognitive load → Mitigation: Clear documentation, examples, adapter patterns

[Breaking] New `Tool` trait must be compatible with existing `ExecutableTool` → Mitigation: Adapter wrapper, incremental migration

[Lifetime] `Arc<dyn Tool>` requires careful ownership management → Mitigation: RAII patterns, explicit `drop` semantics in tests

[Thread safety] `Send + Sync` bounds on `Tool` trait restrict some tool implementations → Mitigation: Document requirements, provide `!Send` alternative path

## Migration Plan

**Phase 1: Foundation** (this change)
1. Add `ToolProvider` trait to `crates/synthia-agent/src/tools/`
2. Add `ExtensionManager` with `Arc<RwLock<HashMap>>` cache
3. Add adapter for existing static `ToolRegistry`
4. Wire into `AgentRunConfig` builder

**Phase 2: Per-category migration** (subsequent changes)
1. Create `FileToolsProvider` wrapping existing file tools
2. Create `BashToolsProvider` wrapping shell tools
3. Create `MCPToolsProvider` for MCP tools
4. Retire static registry calls

**Phase 3: Cleanup** (final)
1. Remove `StaticToolAdapter`
2. Remove legacy `register_defaults()`
3. Pure dynamic-only path

**Rollback:** If issues found, revert to `StaticToolAdapter` wrapping static tools. No data migration needed.

## Open Questions

1. **Provider priority**: If two providers register the same tool name, which wins? (Decision: Last-registered wins, with warning)

2. **Tool deprecation**: How to mark a tool as deprecated but still available? (Decision: Add `deprecated: Option<String>` to `ToolDefinition`)

3. **Schema versioning**: When a tool's schema changes, existing sessions may break? (Decision: Schema version not tracked; breaking changes get new tool name)

4. **Event filtering**: Should `ToolProvider::on_event()` receive all events or filtered subset? (Decision: Filtered to `tool_*` events only to reduce noise)
