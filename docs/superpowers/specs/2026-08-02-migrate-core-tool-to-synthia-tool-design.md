# Migrate `synthia-core::tool` into `synthia-tool` — Design

> **Status:** Draft (post-brainstorming, awaiting user review)
> **Date:** 2026-08-02
> **Scope:** Hard-delete `crates/synthia-core/src/tool/`. Move all 14 child files plus the 167 embedded tests into `crates/synthia-tool/src/`. Resolve three pre-existing double-definitions by collapsing onto the `synthia-tool` side as the single source of truth. Update 11 downstream call-sites in `synthia-agent`, `synthia-server`, and `synthia-cli` (root workspace).

---

## 1. Background

`synthia-core` was intended to be a small utilities crate — IDs, time, paths, error schemas, generic registry patterns. In practice the `synthia-core::tool` submodule has grown to 14 child files totalling ~210 KB and houses the entire tool subsystem: a `Tool` trait, a `ToolRegistry` with generation/identity/scope tracking, a fragment registry, a skill registry, a plugin registry, a rollout tracker, an extension-registry aggregator, and a set of built-in skills and fragments.

This is a layering inversion. The tool subsystem is the *consumer* of `synthia-core` (it needs ULIDs, paths, error types, the generic `Registry`/`LifecycleRegistry` pattern), but today the subsystem is physically *inside* `synthia-core`. As a result:

1. **Every downstream crate must depend on `synthia-core` even if all it wants is the tool surface.** `synthia-tool` already exists and is the documented home of "Tool registry, executor, and sub-trait composition" per the root README.
2. **There is a soft split**: `synthia-server` and `synthia-agent` already import from *both* `synthia-core::tool::*` and `synthia_tool::*` (the latter in `~30` files). The codebase is mid-migration but the file move was never finished.
3. **Three double-definitions exist** that have to be resolved to land the move:
   - `Tool` trait: 3-method version in `core::tool::descriptor` (unused) vs 7-method version in `synthia_tool::traits` (implemented by all 9 builtin tools).
   - `ToolRegistry`: 46 KB version in `core::tool::registry` (generation/identity/scope + dispatch) vs 9 KB version in `synthia_tool::registry` (in-process dispatch catalog).
   - `OutputBound` + 3 enums (`ToolCategory`, `ExecutionMode`, `ToolOutput`): file-level near-duplicate and double-defined enums; the `synthia_tool` side already self-documents as the canonical version.

This spec records the agreed design to (a) complete the file move, (b) collapse the three double-definitions, and (c) leave `synthia-core` as a lean utilities crate.

## 2. Goals & Non-Goals

### Goals

1. `crates/synthia-core/src/tool/` does not exist after the migration. `synthia-core` retains only utilities: `api`, `error`, `filesystem`, `id`, `json_schema`, `path`, `registry` (generic), `sensitive`, `text`, `time`, `token`.
2. The tool subsystem — trait, registries, descriptors, fragments, skills, plugins, rollout, extension, subagent, output-bound, tool-name, capability, provider, built-in skills, built-in fragments — lives entirely in `crates/synthia-tool/`.
3. The three pre-existing double-definitions are collapsed onto the `synthia-tool` side as the single source of truth. The 7-method `Tool` trait, the 46 KB `ToolRegistry`, and the `synthia_tool::truncate::OutputBound` / `synthia_tool::ToolCategory` / `synthia_tool::ExecutionMode` / `synthia_tool::ToolOutput` definitions are canonical.
4. All 167 embedded tests in the moved files continue to pass under their new module path.
5. `cargo check`, `cargo clippy --all-targets --all-features --tests --all`, and `cargo +nightly fmt --all` pass workspace-wide.
6. `cargo test -p <crate>` passes for every crate in the dependency closure (executed per crate, not via `cargo test --workspace`, per project Rust conventions).
7. No new circular dependencies are introduced (verified by post-migration `cargo tree`).

### Non-Goals

1. **No new crate.** We do not split the tool subsystem into a third `synthia-extension` crate. (Considered; deferred — see brainstorming record.)
2. **No re-export shim.** A soft migration that leaves `pub use synthia_tool::*` inside `synthia_core::tool` was considered and rejected. This is a hard migration.
3. **No renaming of public types beyond what's forced by collision resolution.** Public types keep their names (`ToolName`, `ExtensionRegistry`, `ToolRegistry`, `RolloutTracker`, `Skill`, `SkillRegistry`, `Plugin`, `PluginRegistry`, `FragmentRegistry`, `ContextFragment`, `OutputBound`, `ToolCapabilities`, `ToolProvider`, `SubagentFactory`, etc.). Only the **module path** changes (e.g. `synthia_core::tool::rollout::RolloutTracker` → `synthia_tool::rollout::RolloutTracker`).
4. **No refactor of internal logic.** The move is a file relocation plus import-path updates. Internal code structure is preserved except where collision resolution forces it (Section 5).
5. **No documentation prose expansion.** Doc-comments are preserved as-is, except for the two historical references in `descriptor.rs` ("legacy 11-method `Tool` in `synthia-tool`") and `output_bound.rs` ("synthia-tool-orchestrator") which become stale after the move and are corrected to one short sentence each (Section 8).

## 3. User-Confirmed Decisions (Brainstorming Output)

| # | Decision | Choice | Rationale |
|---|---|---|---|
| 1 | Migration strategy | **A. Hard migration** | User explicit preference. Delete `synthia-core::tool`; update 11 call-sites. |
| 2 | `Tool` trait double-definition (REVISED) | **A1. Keep `synthia_tool::Tool` (7-method) as the single canonical trait; delete `core::descriptor::Tool` (3-method).** Absorb the good structural pieces (`ToolInput`/`ToolMetadata`/`ToolError`/`ToolContext`/`ToolDescriptor`/`ToolExample`/`ToolProvenance`/`ContextSource`/`ToolExposure`/`CancelBehavior`/`ToolCapabilities`/`CapabilityBroker`) from `core::descriptor` into `synthia_tool::traits` (or a new `descriptor.rs`). 46KB `ToolRegistry` 内部 9 处 `Arc<dyn Tool>` 通过新增的 `UnifiedToolAdapter` (Section 5.5) 桥接到 7-method trait —— 9 个 builtin 工具实现不动。 | 7-method trait 是事实上的 trait（9 个 builtin 都用它）。3-method trait 实际被 46KB `ToolRegistry` 和 `provider.rs` 用 9 处（registry.rs 7 处 + provider.rs 1 处 + plugin_registry.rs 1 处），不能简单删除。桥接方案：synthia_tool 内部新增 adapter struct，把 7-method `Tool` 包装为 3-method 语义。 |
| 3 | `ToolRegistry` double-definition | **B1. Keep the `ToolRegistry` name on the 46 KB version (core); absorb 9 KB dispatch logic into it. The 46 KB `ToolRegistry` (with `ToolGeneration`/`ToolIdentity`/`RegistrationToken`/`Materialization`/`StaleOrUnknown`/`RegistrationScope`) becomes `synthia_tool::registry::ToolRegistry`.** | `ToolRegistry` is the natural name for a registry. The 46 KB version is the lifecycle/identity/scope system — the more substantive of the two. The 9 KB version's `register`/`snapshot`/`run_with_context`/`execute_tools` methods are folded in (Section 5.2). |
| 4 | `OutputBound` + 3 enum double-definitions | **C1. `synthia-tool` side is the single source of truth.** Delete `core::output_bound`; delete `core::descriptor::{ToolCategory, ExecutionMode, ToolOutput, Tool}` definitions. | `synthia_tool::truncate::OutputBound` doc-comment already self-documents as "migrated from synthia-core"; `synthia_tool::ToolCategory` doc-comment says "mirrors `synthia_core::tool::descriptor::ToolCategory`". The tool-side `ToolOutput` is richer (`content`/`is_error: Option`/`metadata`/`truncated_by` vs core's bare `is_error: bool`) and is what all 9 builtin tools produce. |

## 4. Architecture (Post-Migration)

### 4.1 `synthia-core` — before vs after

**Before** (current): `pub mod api, error, filesystem, id, json_schema, path, registry, sensitive, text, time, token, tool;` (12 modules; `tool` is 14 child files).

**After** (target): `pub mod api, error, filesystem, id, json_schema, path, registry, sensitive, text, time, token;` (11 modules). `synthia-core` is purely a utilities crate.

The generic `registry` module (`Registry`, `LifecycleRegistry`, `RegistryItem`, `EmptyFilter`) **stays** in `synthia-core` because it is parameterized over arbitrary item types by `synthia-tool`, `synthia-skill`, `synthia-provider`, and `synthia-hook` — moving it would force a backwards `synthia-tool` → `synthia-core` direction that does not currently exist.

### 4.2 `synthia-tool` — new module layout

```
crates/synthia-tool/src/
├── lib.rs                       (modified: add new `pub mod`s + re-exports)
├── traits.rs                    (modified: keep 7-method `Tool` trait + absorb core::descriptor's ToolInput/ToolMetadata/ToolError/ToolContext/ToolDescriptor/ToolExample/ToolProvenance/ContextSource/ToolExposure/CancelBehavior/ToolCapabilities/CapabilityBroker/ExecutionMode)
├── types.rs                     (modified: keep rich `ToolOutput`, `Context`, `DispatchMode`, `TruncatedBy`, `Result`, `Error`)
├── events.rs                    (unchanged: `FileChangeEvent`)
├── provider.rs                  (NEW: `ToolProvider` trait + `ToolEvent` enum from core::provider)
├── tool_name.rs                 (NEW: `ToolName` struct from core::tool_name)
├── capability.rs                (NEW: `ToolCapabilities` + `CapabilityBroker` from core::capability; absorb into `traits.rs` vs. standalone — see Section 11 #2)
├── registry/
│   ├── mod.rs                   (modified: add re-exports for new types)
│   ├── metadata.rs              (unchanged: `ToolFilter`)
│   └── registration/
│       ├── mod.rs               (unchanged)
│       ├── entry.rs             (unchanged: `ToolEntry`)
│       ├── registry.rs          (REPLACED: 46 KB core::registry content + 9 KB current dispatch methods merged)
│       ├── registry_trait.rs    (unchanged: `impl Registry<ToolEntry> for ToolRegistry`)
│       └── tests.rs             (unchanged)
├── sub_traits/
│   ├── mod.rs                   (unchanged)
│   ├── category.rs              (unchanged: `ToolCategory` — now the only definition)
│   ├── definition.rs            (unchanged: `ToolDefinition` + `ToolMetadataSnapshot`)
│   ├── execution.rs             (unchanged: `ToolExecution`)
│   └── lifecycle.rs             (unchanged: `ToolLifecycle`)
├── truncate/                    (unchanged; this is already the canonical `OutputBound`)
│   ├── mod.rs
│   ├── output_bound.rs
│   └── bound_output.rs
├── fragment/                    (NEW directory)
│   ├── mod.rs                   (NEW: `ContextFragment` trait + `FragmentContext`/`FragmentError`/`FragmentRegistry` from core::fragment)
│   └── builtin_fragments.rs     (NEW: from core::builtin_fragments)
├── skill/                       (NEW directory)
│   ├── mod.rs                   (NEW: `Skill` trait + `SkillProvenance`/`SkillError`/`SkillActivation`/`SkillRegistry` from core::skill_registry)
│   └── builtin_skills.rs        (NEW: `CodingSkill`/`SearchSkill`/`DebugSkill`/`BUILTIN_SKILLS`/`detect_invocation_keywords` from core::builtin_skills)
├── plugin.rs                    (NEW: from core::plugin_registry)
├── extension.rs                 (NEW: from core::extension_registry)
├── rollout.rs                   (NEW: from core::rollout)
├── subagent.rs                  (NEW: from core::subagent)
├── builtin/                     (unchanged: builtin tool implementations)
│   ├── mod.rs
│   ├── glob.rs, grep.rs, multi_edit.rs, path.rs, read.rs, shell.rs, web.rs, write.rs
│   ├── apply_patch/
│   └── v4a/
├── tool_test.rs                 (unchanged)
└── types_test.rs                (unchanged)
```

### 4.3 Dependency graph — before vs after

**Before:**
```
synthia-core (root leaf)
synthia-telemetry → synthia-core
synthia-provider → synthia-core, synthia-telemetry
synthia-hook     → synthia-provider, synthia-core
synthia-tool     → synthia-core, synthia-provider
synthia-skill    → synthia-tool, synthia-provider, synthia-core
synthia-session  → synthia-provider, synthia-core
synthia-context  → synthia-skill, synthia-provider, synthia-core
synthia-agent    → synthia-tool, synthia-session, synthia-context, synthia-hook, synthia-provider, synthia-core, synthia-telemetry
synthia-a2a      → synthia-agent, synthia-provider, synthia-tool
synthia-server   → synthia-agent, synthia-a2a, synthia-tool, synthia-context, synthia-session, synthia-hook, synthia-provider, synthia-core, synthia-telemetry
test-support     → synthia-tool, synthia-skill, synthia-context, synthia-hook, synthia-provider, synthia-core
synthia-cli (own ws) → synthia-agent, synthia-tool, synthia-skill, synthia-session, synthia-context, synthia-hook, synthia-provider, synthia-core
```

**After:** identical graph. No edge is added, removed, or reversed. The `synthia-core → synthia-tool` edge that a soft-migration shim would require is **not** introduced; instead, downstream code updates its import paths.

This was verified by the brainstorming dependency-graph audit (task `bg_42daae7f`): every consumer of `synthia_core::tool::X` already has a direct `synthia-tool` dependency.

## 5. Collision Resolution

### 5.1 `Tool` trait and `descriptor` types (Decision A1, REVISED)

- **Keep:** `synthia_tool::Tool` (7-method trait in `traits.rs`) — this is the trait all 9 builtin tools implement.
- **Delete:** `synthia_core::tool::descriptor::Tool` (3-method trait). The 3-method trait is used in 9 places in the 46 KB `ToolRegistry` / `provider.rs` / `plugin_registry.rs` (`Arc<dyn Tool>` and `provider.get_tool() -> Option<Arc<dyn Tool>>`). After deletion those 9 sites switch to a new `UnifiedToolAdapter` (Section 5.5).
- **Absorb** from `synthia_core::tool::descriptor.rs` into `synthia_tool::traits.rs` (or a new `synthia_tool::descriptor.rs` if size requires split — the plan step will decide):
  - `ToolInput` (struct)
  - `ToolMetadata` (struct)
  - `ToolError` (enum) — **see Section 5.4 for collision with `synthia_tool::types::Error`**
  - `ToolContext` (struct)
  - `ToolDescriptor` (struct)
  - `ToolExample` (struct)
  - `ToolProvenance` (enum)
  - `ContextSource` (enum)
  - `ToolExposure` (enum)
  - `CancelBehavior` (enum)
  - `ToolCategory` (enum) — **already exists in `synthia_tool::sub_traits::category.rs`; core version is identical and is dropped (Decision C1)**
  - `ExecutionMode` (enum) — **already exists in `synthia_tool::traits.rs`; core version is identical and is dropped (Decision C1)**
  - `ToolCapabilities` (struct, 8 bool flags) + `CapabilityBroker` (wrapper)

**Layout choice** (resolved in the plan step, not here): if absorbed types fit in `traits.rs` without pushing it past 250 LOC, keep them in `traits.rs`. Otherwise create `crates/synthia-tool/src/descriptor.rs` for the structural types and re-export from `traits.rs` so the trait co-locates with its I/O shapes.

### 5.2 `ToolRegistry` (Decision B1)

The 46 KB `synthia_core::tool::registry::ToolRegistry` is the canonical name. The 9 KB `synthia_tool::registry::registration::registry::ToolRegistry` is the dispatch catalog.

**Merge rule:**
1. `synthia_tool::registry::registration::registry::ToolRegistry` is **replaced** by the 46 KB content.
2. The 9 KB file's methods are absorbed as follows:
   - `register(item: ToolEntry) -> Result<(), AlreadyExists>` — folded in alongside the 46 KB version's `register(provider, ToolEntry)`. The 46 KB version takes a `ToolProvider`; the 9 KB version takes a `ToolEntry` directly. **Both methods survive** under the names `register` and `register_entry` (or one becomes `register_raw`). The plan step will pick the final signature; the principle is: callers using `ToolEntry` directly (the in-process builtin case) and callers using `ToolProvider` (the dynamic-discovery case) both have an entry point.
   - `snapshot()` (9 KB returns `Vec<ToolMetadataSnapshot>`) and `snapshot() -> Materialization` (46 KB) — both kept. The 9 KB variant is renamed `metadata_snapshots()` to disambiguate.
   - `run_with_context(...)` and `execute_tools(...)` (9 KB dispatch logic) — folded in as private dispatch methods on the 46 KB `ToolRegistry`. The 9 KB version contained the actual call-and-stream loop that the 46 KB version is missing; without it the new `ToolRegistry` cannot serve tool calls.
   - `contains()` / `len()` / `is_empty()` (9 KB) — folded in unchanged.
3. `ToolEntry` (currently `synthia_tool::registry::registration::entry::ToolEntry`) **stays as-is** in its current location. The 46 KB core's `pub(crate) struct ToolEntry` is replaced by the public `synthia_tool::ToolEntry`. The 46 KB `ToolEntry` had a different shape (provider-token-scoped); the 9 KB version's shape is preserved because that's what `Arc<dyn Tool>` consumers expect.
4. The 46 KB's `pub(crate)` `ToolEntry` is **removed entirely**; the 9 KB `synthia_tool::ToolEntry` is now the only `ToolEntry` and is `pub`.
5. `synthia_tool::registry::registration::registry.rs` is the file that **physically holds** the merged 46 KB + 9 KB content. Its path is `crates/synthia-tool/src/registry/registration/registry.rs`. After the merge it will be ~46 KB + ~9 KB = ~55 KB. If the project convention is 250 LOC ceiling, the plan step will split it: `registry.rs` (lifecycle/identity/scope) + `dispatch.rs` (run_with_context/execute_tools). The split boundary is the 9 KB vs 46 KB boundary.
6. All other 46 KB types (`ToolGeneration`, `ToolIdentity`, `RegistrationToken`, `RegistrationError`, `Materialization`, `StaleOrUnknown`, `RegistrationScope`) are kept verbatim in their new home.

### 5.3 `OutputBound` and 3 enums (Decision C1)

- **Delete** `synthia_core::tool::output_bound.rs` (9.3 KB). The file is a near-byte-identical duplicate of `synthia_tool::truncate::output_bound.rs`. The tool-side file's `truncate/mod.rs` already self-documents as "migrated from synthia-core"; we are completing that migration.
- **Delete** the `ToolCategory` enum in `synthia_core::tool::descriptor.rs` (10 variants, identical to the `synthia_tool::sub_traits::category::ToolCategory`).
- **Delete** the `ExecutionMode` enum in `synthia_core::tool::descriptor.rs` (Parallel/Sequential, identical to the `synthia_tool::traits::ExecutionMode`).
- **Delete** the `ToolOutput` struct in `synthia_core::tool::descriptor.rs` (bare `is_error: bool` — the tool-side `synthia_tool::types::ToolOutput` is the superset).
- **Keep** the `synthia_tool` side of all four as the single source of truth.

### 5.4 `ToolError` vs `synthia_core::Error` (open — plan step)

`crates/synthia-core/src/tool/descriptor.rs` defines a `pub enum ToolError` distinct from the generic `synthia_core::Error`. `synthia_tool::types.rs` re-exports `synthia_core::Error` as its error type.

The plan step will read every call-site of `ToolError` and decide:
- If `ToolError` is just a wrapper / category enum and all variants are reachable via `synthia_core::Error`, fold into `synthia_core::Error`.
- If `ToolError` has variants that do not exist in `synthia_core::Error`, keep it as a distinct type at `synthia_tool::ToolError` and re-export from the `traits`/`descriptor` module.

This decision is deferred to the plan because it requires reading the 167-test corpus to see the call patterns, not just the type definition. **The spec constrains the outcome to one of those two options, not a third.**

### 5.5 `UnifiedToolAdapter` — bridging 46 KB `ToolRegistry` to the 7-method `Tool` trait (NEW)

The 46 KB `ToolRegistry` (`synthia_core::tool::registry`) and `provider.rs` use the 3-method `Tool` trait in 9 places:

| File | Count | Sites |
|---|---|---|
| `crates/synthia-core/src/tool/registry.rs` | 7 | L47 `pub(crate) tool: Arc<dyn Tool>`; L58 `tools: HashMap<ToolName, Arc<dyn Tool>>` (in `Materialization`); L146 `let resolved: Vec<(ToolDescriptor, Arc<dyn Tool>)>`; L279 `Result<Arc<dyn Tool>, StaleOrUnknown>`; L312 `Option<Arc<dyn Tool>>`; L579 and L627 `Arc::new(TestTool { ... })` (test only) |
| `crates/synthia-core/src/tool/provider.rs` | 1 | L22 `get_tool() -> Option<Arc<dyn crate::tool::descriptor::Tool>>` |
| `crates/synthia-core/src/tool/plugin_registry.rs` | 1 | L697 `get_tool() -> Option<Arc<dyn Tool>>` |

The 3-method trait is deleted by Section 5.1. To keep the 46 KB `ToolRegistry` working without rewriting its 9 call-sites, the migration introduces a new private adapter inside `synthia-tool`:

**New file:** `crates/synthia-tool/src/registry/registration/adapter.rs`

```rust
//! Adapter wrapping a 7-method `Tool` so the 46 KB `ToolRegistry`
//! can hold it as a 3-method-style tool.

pub struct UnifiedToolAdapter {
    inner: Arc<dyn Tool>,                        // 7-method
    descriptor: ToolDescriptor,                  // cached on construction
}

impl UnifiedToolAdapter {
    pub fn new(inner: Arc<dyn Tool>, descriptor: ToolDescriptor) -> Self {
        Self { inner, descriptor }
    }

    pub fn name(&self) -> &str { self.descriptor.name.full_name().as_str() }
    pub fn descriptor(&self) -> &ToolDescriptor { &self.descriptor }

    /// Bridges 3-method `execute(ToolInput, &ToolContext) -> Result<ToolOutput, ToolError>`
    /// to 7-method `call(serde_json::Value, &Context) -> ToolOutput`.
    pub async fn execute(
        &self,
        input: ToolInput,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        // Build synthia_tool::Context from ToolContext
        let sctx = Context {
            session_id: ctx.session_id.clone(),
            workspace_root: ctx.workspace_root.clone(),
            output_bound: None,   // 46 KB registry path does not use bound
            ..Context::default()
        };
        // Delegate to 7-method call
        let out = self.inner.call(input.raw.clone(), &sctx).await;
        // Convert ToolOutput error flag to ToolError
        if out.is_error.unwrap_or(false) {
            let msg = out.text().unwrap_or_default();
            Err(ToolError::ExecutionFailed(msg))
        } else {
            Ok(out)
        }
    }
}
```

**Rules for the migration:**

1. The adapter is a `pub` type but its module path is private to `synthia_tool::registry::registration`. It is **not** re-exported from `synthia_tool::*` — it is implementation detail.
2. Every `Arc<dyn Tool>` in the moved 46 KB file becomes `Arc<UnifiedToolAdapter>`.
3. Every `provider.get_tool(name) -> Option<Arc<dyn Tool>>` (provider.rs L22, plugin_registry.rs L697) becomes `provider.get_tool(name) -> Option<Arc<UnifiedToolAdapter>>`. This requires `ToolProvider::get_tool` to wrap the returned 7-method `Tool` in an `UnifiedToolAdapter` before returning.
4. `Materialization.tools: HashMap<ToolName, Arc<dyn Tool>>` becomes `HashMap<ToolName, Arc<UnifiedToolAdapter>>`. The 46 KB code's `tool.descriptor()` calls (line 90) become `adapter.descriptor()`.
5. The two test-only `Arc::new(TestTool { ... })` constructions (L579, L627) become `Arc::new(UnifiedToolAdapter::new(Arc::new(TestTool { ... }), test_descriptor()))` — the local `TestTool` struct (which implements the 3-method trait) is replaced by a local struct that implements the 7-method `Tool` trait. The test bodies otherwise stay the same.
6. `UnifiedToolAdapter::execute` does the only input/ctx/output conversion in the 46 KB code path. There is one place to maintain; the rest of the 46 KB code keeps its current `Result<ToolOutput, ToolError>` shape.

**Net effect:** the 46 KB `ToolRegistry` semantics are preserved. The 9 builtin tools continue to implement the 7-method `Tool` trait. The adapter is the boundary.

**Open sub-decision** (decided in plan step): does the adapter store the cached `ToolDescriptor` as a `ToolDescriptor` field, or does it compute it lazily from the 7-method `Tool` (by reading `description()` + `parameters()` + constructing the rest)? The plan step picks one based on whether all 9 builtin tools can construct a `ToolDescriptor` cheaply.

## 6. File Move Manifest (14 files)

| Source (delete) | Destination (create) | Lines | Tests |
|---|---|---|---|
| `crates/synthia-core/src/tool/mod.rs` | (deleted; contents move to `synthia-tool/src/lib.rs` re-exports) | 31 | 0 |
| `crates/synthia-core/src/tool/tool_name.rs` | `crates/synthia-tool/src/tool_name.rs` | 301 | 14 |
| `crates/synthia-core/src/tool/capability.rs` | `crates/synthia-tool/src/capability.rs` (or merged into `traits.rs`; see Section 11 #2) | layout decided in plan step | 0 |
| `crates/synthia-core/src/tool/descriptor.rs` | `crates/synthia-tool/src/descriptor.rs` (or merged into `traits.rs`); **excluding** the 4 dropped types (Section 5.1) | 6.7 KB → smaller after drops | 0 |
| `crates/synthia-core/src/tool/provider.rs` | `crates/synthia-tool/src/provider.rs` | 868 B | 0 |
| `crates/synthia-core/src/tool/registry.rs` | `crates/synthia-tool/src/registry/registration/registry.rs` (replaces existing 9 KB file) | 46.8 KB + 9 KB = ~55 KB after merge | 31 |
| `crates/synthia-core/src/tool/fragment.rs` | `crates/synthia-tool/src/fragment/mod.rs` (new directory) | 15.9 KB | 12 |
| `crates/synthia-core/src/tool/builtin_fragments.rs` | `crates/synthia-tool/src/fragment/builtin_fragments.rs` | 15.8 KB | 15 |
| `crates/synthia-core/src/tool/skill_registry.rs` | `crates/synthia-tool/src/skill/mod.rs` (new directory) | 14.3 KB | 12 |
| `crates/synthia-core/src/tool/builtin_skills.rs` | `crates/synthia-tool/src/skill/builtin_skills.rs` | 16.2 KB | 25 |
| `crates/synthia-core/src/tool/plugin_registry.rs` | `crates/synthia-tool/src/plugin.rs` | 46.3 KB | 32 |
| `crates/synthia-core/src/tool/extension_registry.rs` | `crates/synthia-tool/src/extension.rs` | 12.7 KB | 8 |
| `crates/synthia-core/src/tool/output_bound.rs` | **DELETED** (file-level duplicate, Decision C1) | 9.3 KB | 7 (lost — covered by existing `synthia_tool::truncate::output_bound.rs` tests) |
| `crates/synthia-core/src/tool/rollout.rs` | `crates/synthia-tool/src/rollout.rs` | 12.1 KB | 11 |
| `crates/synthia-core/src/tool/subagent.rs` | `crates/synthia-tool/src/subagent.rs` | 1.7 KB | 0 |

**Test count: 167 test functions** move with their source files. The 7 `output_bound` tests are not lost — they are already covered by `synthia_tool::truncate::output_bound.rs`'s own tests (the file is a near-duplicate).

## 7. Downstream Call-Site Updates (11 files)

| File | Change |
|---|---|
| `crates/synthia-agent/src/agent.rs` | L4: `synthia_core::tool::extension_registry::ExtensionRegistry` → `synthia_tool::extension::ExtensionRegistry` |
| `crates/synthia-agent/src/loop_context.rs` | L4: `synthia_core::tool::registry::RegistrationScope` → `synthia_tool::registry::RegistrationScope` |
| `crates/synthia-agent/src/loop_services.rs` | L11: `synthia_core::tool::rollout::RolloutTracker` → `synthia_tool::rollout::RolloutTracker`. L49, L71, L186: inline `synthia_core::tool::OutputBound` → `synthia_tool::truncate::OutputBound` |
| `crates/synthia-agent/src/component_assembly.rs` | L7-10: `synthia_core::tool::{extension_registry::*, fragment::*}` → `synthia_tool::{extension::*, fragment::*}`. L112: `synthia_core::tool::registry::ToolRegistry::new()` → `synthia_tool::registry::ToolRegistry::new()`. Drop `as CoreToolRegistry` alias |
| `crates/synthia-agent/src/config/agent_config/run_config.rs` | L7-10: `synthia_core::tool::{extension_registry::*, rollout::*}` → `synthia_tool::{extension::*, rollout::*}` |
| `crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs` | L14-17 and L1273-1279: rewrite both `use synthia_core::tool::{...}` blocks to `synthia_tool::*` paths |
| `crates/synthia-agent/src/stream_builder/builder/tool_execution/execute.rs` | L89: inline `&synthia_core::tool::OutputBound` → `&synthia_tool::truncate::OutputBound` |
| `crates/synthia-server/src/session/controller.rs` | L30-33: `synthia_core::tool::{extension_registry::*, rollout::*}` → `synthia_tool::{extension::*, rollout::*}` |
| `crates/synthia-server/src/state/app_state.rs` | L12-19: 8 submodules → `synthia_tool::*` paths. Drop `as CoreToolRegistry` alias. L169-173 and L278-282: `synthia_core::tool::builtin_skills::*` → `synthia_tool::skill::builtin_skills::*`. L176/181/186 and L285/290/295: 6 inline `Arc<dyn synthia_core::tool::skill_registry::Skill>` → `Arc<dyn synthia_tool::skill::Skill>` |
| `crates/synthia-server/src/routes/skills.rs` | L271: doc-comment text update |
| `crates/synthia-server/tests/e2e_registry_pipeline_test.rs` | L236: inline `synthia_core::tool::fragment::FragmentContext::new(...)` → `synthia_tool::fragment::FragmentContext::new(...)` |
| `synthia-cli/src/repl_core/repl/agent_message.rs` | L20: `synthia_core::tool::extension_registry::ExtensionRegistry` → `synthia_tool::extension::ExtensionRegistry` |

`crates/synthia-skill`, `test-support`, `synthia-context`, `synthia-session`, `synthia-provider`, `synthia-hook`, `synthia-telemetry`, `synthia-a2a`, `synthia-cache-mark` have **zero** `synthia_core::tool::*` imports and require no changes.

## 8. Documentation Comment Updates

Two historical doc-comment references become stale after the move and are corrected:

1. `synthia-core/src/tool/descriptor.rs` line 105 (was: "The legacy 11-method `Tool` trait in `synthia-tool` is deprecated."): This file is deleted by Section 5.1, so the comment is removed with it. **No update needed.**
2. `synthia-core/src/tool/output_bound.rs` (was: doc-comment referenced `synthia-tool-orchestrator`): This file is deleted by Section 5.3, so the comment is removed with it. **No update needed.**
3. `synthia-tool/src/sub_traits/category.rs` lines 4 and 8 (currently: `//! Mirrors synthia_core::tool::descriptor::ToolCategory`): Update to `//! The canonical ToolCategory for Synthia tool categorization.` and drop the "mirrors" wording.

No other doc-comments are affected because the move is path-preserving — the types' new home is `synthia_tool::X` instead of `synthia_core::tool::X`, but the doc-comment prose in the moved files does not reference the old path.

## 9. Verification Strategy

Per `AGENTS.md` and `.trae/rules/rust.md`:

```bash
# Per-crate checks (NOT --workspace, per project convention)
cargo check -p synthia-core
cargo check -p synthia-tool
cargo check -p synthia-agent
cargo check -p synthia-server
cargo check -p synthia-cli   # root workspace
cargo check -p synthia-skill
cargo check -p synthia-context
cargo check -p synthia-session
cargo check -p synthia-provider
cargo check -p synthia-hook
cargo check -p synthia-telemetry
cargo check -p synthia-a2a
cargo check -p synthia-cache-mark
cargo check -p test-support

# Per-crate tests (NOT --workspace, per project convention)
cargo test -p synthia-core
cargo test -p synthia-tool
cargo test -p synthia-agent
cargo test -p synthia-server
cargo test -p synthia-cli    # root workspace
cargo test -p synthia-skill
cargo test -p synthia-context
cargo test -p test-support

# Workspace lint
cargo clippy --all-targets --all-features --tests --all

# Workspace format
cargo +nightly fmt --all

# Cycle confirmation
cargo tree --workspace --no-default-features | grep -E 'synthia-(core|tool|agent|server)'

# Cleanup if disk pressure hits
cargo clean
```

**Success criteria** (all must hold):
- Every `cargo check` and `cargo test` above exits 0.
- `cargo clippy --all-targets --all-features --tests --all` exits 0 with no warnings.
- `cargo +nightly fmt --all -- --check` exits 0.
- `cargo tree` shows no new edges (compare against pre-migration graph in Section 4.3).
- `git grep "synthia_core::tool::" -- 'crates/*' 'synthia-cli/*' 'test-support/*'` returns zero matches in source files (only doc-comments in `synthia-tool/src/sub_traits/category.rs` may remain until the post-migration cleanup task).

## 10. Rollback Plan

This is a hard migration. The pre-migration state is preserved by:
1. **Single PR / single commit** containing all changes. If `cargo check -p synthia-core` fails, revert the commit and the workspace is restored.
2. The plan step will instruct the implementer to commit *before* deleting the `synthia-core/src/tool/` directory so the previous commit has the full source still in place. The deletion is its own commit.
3. If only a partial rollback is needed (e.g. the 46 KB `ToolRegistry` merge has unexpected compilation errors), the file move is complete and atomic — only the 46 KB/9 KB merge has refactor surface.

## 11. Open Items (Resolved in Plan Step)

1. **Where does `ToolError` end up?** Section 5.4 — depends on call-site analysis done in the plan step.
2. **Does `descriptor.rs` get absorbed into `traits.rs` or split into a separate `descriptor.rs`?** Section 5.1 — depends on LOC ceiling compliance, decided in the plan step.
3. **Does the merged `ToolRegistry` stay in one file or split into `registry.rs` + `dispatch.rs`?** Section 5.2 — depends on 250 LOC ceiling, decided in the plan step.
4. **Does `provider::ToolEvent` merge into `events::FileChangeEvent` or stay separate?** Needs call-site read in the plan step.
5. **`UnifiedToolAdapter` caches `ToolDescriptor` eagerly or computes lazily?** Section 5.5 — depends on whether all 9 builtin tools can construct a `ToolDescriptor` cheaply on first call.

## 12. Out-of-Scope (Future Work)

- Splitting the tool subsystem into a separate `synthia-extension` crate (brainstorming option C). Deferred.
- Removing the `synthia_tool::subagent::SubagentFactory` trait in favor of a typed `Subagent` enum. Deferred.
- Unifying `ToolDefinition` / `ToolExecution` / `ToolLifecycle` sub-traits into a single `Tool` interface. Deferred — the current 7-method monolithic trait is preserved.
- Adding `no_std` support to any of the moved modules. Out of scope.

---

**Approval gate:** The user has approved the four primary decisions (Section 3) and the five architectural sections (Sections 4.1–4.3, 5.1–5.3, 6, 7, 8, 9, 10) during brainstorming. This spec is now committed to the repo; the user is asked to review the written spec once more before the writing-plans skill produces the implementation plan.
