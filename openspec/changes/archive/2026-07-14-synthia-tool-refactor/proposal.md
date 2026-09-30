# Proposal: synthia-tool-refactor (Change 1 of v3 architecture)

**Date**: 2026-07-12
**Status**: Skeleton, awaiting user approval (no auto-commit, no writer launched)
**Parent design**: [`docs/superpowers/specs/2026-07-12-synthia-v3-tool-first-architecture-design.md`](../../../docs/superpowers/specs/2026-07-12-synthia-v3-tool-first-architecture-design.md) §3.1
**Related in-flight work**:
- [`add-dynamic-tool-provider-system`](../add-dynamic-tool-provider-system/) (Phase 1 1.1-1.4 已合入；Phase 2 R1.1-R1.3 待办 — **全部并入本 Change R6**)
- [`adopt-explore-agent-recommendations`](../adopt-explore-agent-recommendations/) R1-R3（3 个 ToolProvider 实现 — **全部并入本 Change R6**）
- [`9-abstractions-toolification` spec](../specs/9-abstractions-toolification/spec.md)（已 spec；缺落地 — **本 Change R6 部分收口**）
- [`extension-point-matrix`](../specs/extension-point-matrix/spec.md) Tool scope 的 7 个点（已声明未 wire — **本 Change 落地**）

## Why

Synthia's `synthia-tool::Tool` trait uses `#[async_trait]` — **single shape, dyn-incompatible in concrete form, 9 methods mixing UI rendering with execution**. The orchestrator (`synthia-tool-orchestrator/src/lib.rs` 2876 LOC) entangles resolver + approval + sandbox + retry + edit-conflict + events into one struct. Three parallel tool registries coexist: `ToolRegistry` (`synthia-tool::registry`), `LayeredToolRegistry` (`synthia-tool::scoped_registry:208-298`, 491 LOC, **only used by its own tests**), `ScopedToolRegistry` (`synthia-tool::scoped_registry`, scoped with `ScopeGuard` RAII). 60+ extension points are declared in `extension_points/` but **the main loop fires zero of them**. Adding a new tool today requires editing 3 places and recompiling.

Three opencode/codex/pi-mono best practices are not adopted:
1. **Object-safe `ToolExecutor<Invocation>`** (codex `codex-rs/tools/src/tool_executor.rs:49-69`) — RPITIT, no `async_trait`, dyn-compatible, generic over invocation type, `Send + Sync + 'static`.
2. **`ToolRouter` + `ToolRegistry` separation** (codex `codex-rs/core/src/tools/router.rs:34-224`) — router owns *model-visible specs*; registry owns *runtime dispatch*. ToolExposure `{Direct, Deferred, DirectModelOnly, Hidden}` + `ToolSearch` for >50 tools without prompt bloat (codex `codex-rs/tools/src/tool_search.rs:21-66`).
3. **Dual-shape `AgentTool` (lean) + `ExtensionTool` (rich)** (pi-mono `packages/agent/src/types.ts:308-331` and `packages/coding-agent/src/core/extensions/types.ts:426-473`) — orchestrator sees only the 5-method `AgentTool`; rich UI rendering lives on `ExtensionTool`.

## What Changes

**C1.1** New crate `synthia-tool-core` with:
- `ToolInvocation` enum (Function, ToolSearch, Other(Arc<dyn AnyToolInvocation>))
- `ToolExecutor<Invocation>` (object-safe RPITIT)
- `ToolExposure` enum (`Direct, Deferred, DirectModelOnly, Hidden`)
- `ToolSpec` (model-visible) + `LoadableToolSpec` (deferred-discovery view)
- `ToolRegistry` + `ToolRouter`
- `AgentTool` (lean, 5 methods) + `ExtensionTool` (rich, +7 methods)
- `ToolProvider` trait (new — already partly defined in `add-dynamic-tool-provider-system` R1)

**C1.2** `synthia-tool::Tool` legacy trait becomes `#[deprecated(since = "0.2", note = "use AgentTool")]` type alias for `AgentTool` for one minor cycle; blanket adapter (`impl<T: LegacyTool> AgentTool for T`) keeps existing impls compiling.

**C1.3** Three registries collapse: `ToolRegistry` `synthia-tool/src/registry/` deprecated; `LayeredToolRegistry` deleted (only tests consume it); only `ToolRegistry` v2 in `synthia-tool-core` survives, with `ScopeGuard` RAII semantics preserved.

**C1.4** `synthia-tool-orchestrator` splits into:
- `synthia-tool-router` — `{registry, model_visible_specs}` (model-facing)
- `synthia-tool-orchestrator` — approval + retry + lifecycle (rename existing)
- `synthia-tool-runtime` — sandbox-attempt + tool execution

**C1.5** Wire 7 of 9 Tool-scope extension points from `extension-point-matrix`:
- `tool.registry.register`
- `tool.registry.unregister`
- `tool.definition.transform`
- `tool.execution_mode.override`
- `tool.parallelism.barrier`
- `tool.output.format`
- `tool.output.metadata.inject`
- (The remaining 2 — `tool.execute.before`/`tool.execute.after` — wire in **Change 2**)

**C1.6** Migrate 9 existing non-Tool abstractions to `ExtensionTool` (per `9-abstractions-toolification/spec.md`):
- `compact_context_tool` (remove facade)
- `load_skill` (implicit_tool → ExtensionTool)
- `AgentTool` (subagent → task-style Wrap)
- `SELF_REFLECT_TOOL_NAME` (const → ExtensionTool)
- `MonitorTool` (wrap)
- MCP servers (wrap via `MCPToolsProvider`)
- `ExternalHookTool`
- `QuerySkillUsageTool`
- Plugin CLI entries (`kind: Tool`)

**C1.7** Full ToolProvider Roll-out (absorb `add-dynamic-tool-provider-system` R1.1-R1.3 + `adopt-explore-agent-recommendations` R1-R3):
- `FileToolsProvider` (already implemented)
- `BashToolsProvider` (Phase 2)
- `MCPToolsProvider` (Phase 2 — wraps `synthia-mcp::Client::list_tools()`)
- `SearchToolsProvider` (Phase 2 — wraps file/grep/glob tools)
- `register_defaults()` deprecated in favor of `ExtensionManager` from providers

## Capabilities

### New Capabilities

- `ToolExecutor<Invocation>` — object-safe, RPITIT, dyn-compatible, no `async_trait`
- `ToolRouter` + `ToolRegistry` separation
- `ToolExposure` + `ToolSearch` for deferred discovery
- `AgentTool` + `ExtensionTool` dual shape
- `ToolProvider` trait (from `add-dynamic-tool-provider-system` Phase 2 final)
- 4 ToolProvider implementations: File/Bash/MCP/Search
- 9-abstractions-toolification part-1 (compact_context, subagent, guardian, monitor)

## Risks

| Risk | Mitigation |
|------|-----------|
| `#[async_trait]` removal breaks downstream dyn dispatch | Blanket adapter; 1 minor deprecation cycle |
| 9 abstractions wrapped in `Arc<dyn ExtensionTool>` adds indirection | Single wrapper type, no exposed detail |
| `schemars` JSON schema build per `parameters()` is hot | Lazy `OnceCell<Arc<Schema>>` |
| Cache invalidation on `register`/`unregister` race | `AtomicU64::fetch_add(1)` for version |
| `8 * 1500` LOC PR could miss review focus | 7 Rounds, each ≤ 1500 LOC, each independently verifiable |

## Out of Scope (Deferred)

- Compaction tool semantics (Change 2)
- DoomLoop / Permission event-based re-write (Change 2)
- JSONL append-only Session (Change 3)
- Wire Protocol (Change 3)
- `2.2.3 ExternalHookTool` full implementation (Change 2 R7)
- Plugin CLI as Tool integration (Change 3 R8)

## Reference

- Parent design doc: [design.md](../../docs/superpowers/specs/2026-07-12-synthia-v3-tool-first-architecture-design.md)
- Codex pattern: `codex-rs/tools/src/tool_executor.rs:49-69`
- Codex router pattern: `codex-rs/core/src/tools/router.rs:34-224`
- Codex Exposure: `codex-rs/tools/src/tool_search.rs:21-66`
- pi-mono AgentTool: `pi-mono/packages/agent/src/types.ts:308-331`
- pi-mono ToolDefinition: `pi-mono/packages/coding-agent/src/core/extensions/types.ts:426-473`
- opencode Tool.Def: `opencode/packages/opencode/src/tool/tool.ts:55-65`
