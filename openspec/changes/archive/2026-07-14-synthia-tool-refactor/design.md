# Design: synthia-tool-refactor (Change 1 of v3 architecture)

**Date**: 2026-07-12
**Status**: Skeleton, awaiting user approval
**Parent**: [`docs/superpowers/specs/2026-07-12-synthia-v3-tool-first-architecture-design.md`](../../../../docs/superpowers/specs/2026-07-12-synthia-v3-tool-first-architecture-design.md) §3.1

## Context

Synthia has 28 crates and ~39000 LOC. The current `synthia-tool` crate uses `#[async_trait]` with a single 9-method `Tool` trait, `synthia-tool-orchestrator` is a 2876-LOC monolith, and 3 parallel registries coexist (one of which is unused). Adding a new tool requires editing multiple places and recompiling. **Three production agents** — opencode (TS), codex (Rust), pi-mono (TS) — have independently evolved better abstractions.

**Reusable assets** (from in-flight or already-shipped work):
- `crates/synthia-tool/src/scoped_registry.rs` — `ScopedToolRegistry` with `ScopeGuard` RAII cleanup
- `crates/synthia-tool-orchestrator/src/lib.rs:540-910` — retry + lifecycle (3-tier pattern)
- `crates/synthia-agent/src/tools/dynamic_provider/extension_manager.rs` — `ToolProvider` trait + `ExtensionManager` (from `add-dynamic-tool-provider-system` Phase 1)
- `crates/synthia-tool/src/builtin/` — 7 working tool impls that already comply with the `Tool` shape
- `crates/synthia-tool/src/types.rs:50-146` — `ToolInput` + `ToolOutput` + `TruncatedBy` (the `truncated_by` field is unused — fill in bash tool at R5)
- All extension-points declared in `extension-point-matrix` (60+ points, half-typed)

**Hard constraints (must not violate)**:
- P1 (KV-cache prefix consistency)
- P6 (Permission/DoomLoop fail-closed)
- P9 (every `fire_*` emits OTel span)
- No `async_trait` going forward (per codex `AGENTS.md:23-28` — align with Rust RPITIT)
- Type safety: every public surface `Send + Sync + 'static`
- No `unsafe`

## Goals

1. Adopt **object-safe `ToolExecutor<Invocation>`** so dyn dispatch is unblocked.
2. Adopt **ToolRouter + ToolRegistry separation** so model-visible spec and runtime dispatch are independently swappable.
3. Adopt **ToolExposure + ToolSearch** for >50 tool extensibility.
4. Introduce **`AgentTool` (lean) + `ExtensionTool` (rich)** dual shape.
5. Land `ToolProvider` Trait + 4 concrete providers (File/Bash/MCP/Search).
6. Migrate 9 existing non-Tool abstractions to `ExtensionTool` (per `9-abstractions-toolification/spec.md`).
7. Wire 7 Tool-scope extension points from `extension-point-matrix` (the remaining 2 land in Change 2).
8. **Zero behavioral regression** on 5 historical e2e tests.

## Non-Goals

- Event bus / 27 event types (Change 2)
- JSONL session / wire protocol (Change 3)
- DoomLoop / Permission event-driven re-write (Change 2 R2-R3)
- `2.2.3 ExternalHookTool` + `2.3.2 Plugin CLI as Tool` (Change 2 R7 / Change 3 R8)
- Codex-style `code-mode` JS/WASM runtime
- SQLite-derived metadata mirror
- Markdown tool packs (`FileToolPack`/`WebToolPack`) — deferred until `ToolProvider` proves the API

## Architecture

### Module Structure

```
crates/synthia-tool-core/                  # NEW crate (primary landing spot)
├── lib.rs                                  # re-exports
├── invocation.rs                           # ToolInvocation { Function, ToolSearch, Other }
├── executor.rs                             # ToolExecutor<I: Send + 'static> (object-safe RPITIT)
├── exposure.rs                             # ToolExposure { Direct, Deferred, DirectModelOnly, Hidden }
├── spec.rs                                 # ToolSpec, LoadableToolSpec, ToolSearchInfo
├── agent_tool.rs                           # AgentTool (lean, 5 methods)
├── extension_tool.rs                       # ExtensionTool: AgentTool + 7 rich methods
├── provider.rs                             # ToolProvider trait (absorb from dynamic_provider)
├── registry.rs                             # ToolRegistry (3-reg collapse target)
├── router.rs                               # ToolRouter { registry, model_visible_specs, cache }
├── compat.rs                               # Blanket impl: LegacyTool -> AgentTool
├── error.rs                                # ToolError, FunctionCallError
└── tests/                                  # unit + integration

crates/synthia-tool/                       # MODIFIED (becomes a compat shim)
└── (legacy Tool trait -> deprecated type alias)

crates/synthia-tool-router/                # NEW crate (model-visible)
└── lib.rs

crates/synthia-tool-runtime/               # NEW crate (sandbox + execution)
└── lib.rs

crates/synthia-tool-orchestrator/          # SHRUNK (orchestration only, ~1500 LOC)
└── lib.rs
```

### Core Data Structures

```rust
// crates/synthia-tool-core/src/exposure.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolExposure {
    /// Normal tool, always in prompt.
    Direct,
    /// Available but loaded only via ToolSearch.
    Deferred,
    /// Model can invoke; user CLI never shows it (system tools).
    DirectModelOnly,
    /// Hidden from model entirely.
    Hidden,
}

// crates/synthia-tool-core/src/spec.rs
pub struct ToolSpec {
    pub name: ToolName,
    pub description: String,
    pub parameters: Arc<schemars::schema::Schema>,   // built once, lazy
    pub execution_mode: ExecutionMode,
    pub exposure: ToolExposure,
    pub search_keywords: Vec<String>,
}

pub struct LoadableToolSpec {
    pub name: ToolName,
    pub summary: String,
    pub namespace: Option<String>,
    pub tokens_hint: u32,
}

// crates/synthia-tool-core/src/agent_tool.rs
pub trait AgentTool: Send + Sync + 'static {
    fn name(&self) -> &ToolName;
    fn description(&self) -> &str;
    fn execution_mode(&self) -> ExecutionMode;
    fn parameters(&self) -> Arc<schemars::schema::Schema>;
    async fn call(&self, args: serde_json::Value, ctx: &ToolContext)
        -> Result<ToolOutputBox, FunctionCallError>;
}

// crates/synthia-tool-core/src/extension_tool.rs
pub trait ExtensionTool: AgentTool {
    fn extension_api_version(&self) -> &'static str { "v1" }
    fn prompt_snippet(&self) -> Option<String> { None }
    fn prompt_guidelines(&self) -> Option<String> { None }
    fn render_call(&self, args: &serde_json::Value) -> Option<String> { None }
    fn render_result(&self, result: &ToolOutputBox) -> Option<String> { None }
    fn needs_extension_context(&self) -> bool { false }
    async fn bind_extension(&self, _ctx: Arc<dyn AnyExtensionContext>) {}
}

// crates/synthia-tool-core/src/executor.rs (object-safe RPITIT)
pub trait ToolExecutor<I: Send + 'static>: Send + Sync + 'static {
    fn name(&self) -> &ToolName;
    fn spec(&self) -> ToolSpec;
    fn exposure(&self) -> ToolExposure { ToolExposure::Direct }
    fn search_info(&self) -> Option<ToolSearchInfo> { None }
    fn handle<'a>(&'a self, inv: I)
        -> Pin<Box<dyn Future<Output = Result<ToolOutputBox, FunctionCallError>> + Send + 'a>>;
}

// crates/synthia-tool-core/src/registry.rs
pub struct ToolRegistry {
    tools: tokio::sync::RwLock<HashMap<ToolName, Arc<dyn AgentTool>>>,
    providers: tokio::sync::RwLock<Vec<Arc<dyn ToolProvider>>>,
    cache_version: AtomicU64,
}

// crates/synthia-tool-core/src/router.rs
pub struct ToolRouter {
    registry: Arc<ToolRegistry>,
    model_spec_filter: fn(&ToolSpec) -> bool,
    spec_cache: tokio::sync::RwLock<Option<(u64, Vec<ToolSpec>)>>,
}
```

### 7 Implementation Rounds

| Round | Scope | LOC | Files | Verification |
|-------|-------|-----|-------|--------------|
| **R1** | `synthia-tool-core` skeleton | +800 | 1 new crate | `cargo check -p synthia-tool-core` |
| **R2** | `Tool` → `AgentTool`/`ExtensionTool` split + compat blanket | 0 net | 3 modified | 5 historical e2e unchanged |
| **R3** | 3-registry collapse → `ToolRegistry` v2 only | -200 | 4 modified | clippy 0 warnings |
| **R4** | `ToolRouter` + spec cache | +400 | 1 new file + 1 new crate | router spec_cache integration test |
| **R5** | `ToolExposure` + `ToolSearch` + bash `truncated_by` wired | +300 | 3 modified | ToolSearch hit-rate test |
| **R6** | `ExtensionTool` ×9 + 4 providers (absorb add-dynamic-tool-provider-system + adopt-explore-agent-recommendations) | +1200 | 6 modified + 1 new crate | 9-abstractions tested via Tool trait |
| **R7** | `Tool` deprecation marker + wire 7 Tool-scope ext points | 0 | 2 modified + 1 add | 64-point partial-mat test |

### Hard rules per Round

1. **Every `register_*` increments `cache_version`** — invalidates router cache
2. **Every builtin tool gets `Exposure::Direct` default** until R5 picks a policy
3. **No `unsafe`**
4. **No `as any` / `#[allow(async_fn_in_trait)]`** anywhere
5. **Backward-compat**: every existing `impl Tool for X` compiles unchanged via `compat::LegacyTool`

## Migration / Rollback

**On deprecation** (R7):
```rust
// crates/synthia-tool/src/traits.rs
#[deprecated(since = "0.2.0", note = "use synthia_tool_core::AgentTool")]
pub trait Tool: AgentTool {}
```

**On removal** (next major 0.3.0):
- The blanket `impl<T: LegacyTool> AgentTool for T` is removed
- `synthia-tool` crate becomes `pub use synthia_tool_core::*` only

**Rollback path**: revert commits in reverse order; API surface is additive the entire 0.2.x cycle.

## Validation Standard

After every Round:
```bash
cargo +nightly fmt --all
cargo check --workspace --all-features
cargo clippy --workspace --all-targets --all-features --tests --all -- -D warnings
cargo test -p synthia-agent --test react_loop_test --test e2e_llm_test --test e2e_event_sequence_test --test e2e_memory_correctness_test
```

Specific:
- `cargo test -p synthia-tool-core` — all registry/router/exposure tests green
- `cargo test -p synthia-tool-orchestrator` — orchestration suite green
- `cargo test -p synthia-agent --test 9_abstractions` — every spec scenario passes
- 7 Tool-scope extension-point integration test (new) passes

## Open Questions

1. **schemars dependency bringup**: already workspace dep (`Cargo.toml:106`); zero-bringup cost. **Resolved.**
2. **JS-WebAssembly future**: defer indefinitely per anti-goals. **Resolved.**
3. **WASM `ToolProvider`**: defer to future Change; native Rust providers cover 80%. **Resolved.**
4. **`ExtensionTool`'s `bind_extension` async safety**: lock via `tokio::sync::Mutex<Arc<()>>` per-ext, RAII via `Drop`. **Resolved in design.**

## Reference

- Parent design: [design.md](../../../../docs/superpowers/specs/2026-07-12-synthia-v3-tool-first-architecture-design.md)
- Proposal: [proposal.md](../proposal.md)
- Plan: [plan.md](../plan.md)
- Tasks: [tasks.md](../tasks.md)
- Codex patterns: `codex-rs/tools/src/tool_executor.rs:49-69`, `codex-rs/core/src/tools/router.rs:34-224`, `codex-rs/tools/src/tool_search.rs:21-66`
- pi-mono dual shape: `packages/agent/src/types.ts:308-331`, `packages/coding-agent/src/core/extensions/types.ts:426-473`
- opencode Tool.Def: `packages/opencode/src/tool/tool.ts:55-65`
- Absorbed changes: `add-dynamic-tool-provider-system`, `adopt-explore-agent-recommendations`
