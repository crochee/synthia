# Pre-flight Decisions for Tool Migration (Task 1)

> **Status:** Locked (pre-flight, before any code change)
> **Date:** 2026-08-02
> **Plan reference:** `docs/superpowers/plans/2026-08-02-migrate-core-tool-to-synthia-tool.md` (Task 1, Steps 1–5)
> **Spec reference:** `docs/superpowers/specs/2026-08-02-migrate-core-tool-to-synthia-tool-design.md` (Section 11 open items)

This document resolves the 5 sub-decisions the spec deferred to the plan step. Each section records (a) the decision, (b) the evidence, and (c) the downstream task that consumes it.

---

## 1. `ToolError` location — **keep as `synthia_tool::ToolError`**

**Decision:** Keep `ToolError` as a distinct enum at `synthia_tool::descriptor::ToolError`. Do **not** fold into `synthia_core::Error`.

**Evidence:**

- `synthia_core::tool::descriptor::ToolError` (lines 64–86 of `descriptor.rs`) has **8 variants**:
  1. `CapabilityDenied { service: String, need: &'static str }`
  2. `ExecutionFailed(String)`
  3. `Timeout(Duration)`
  4. `InvalidInput(String)`
  5. `NotFound(String)`
  6. `Stale { name: String, seen: u64, current: u64 }`
  7. `Cancelled`
  8. `PermissionDenied(String)`

- `synthia_core::Error` (re-exported as `synthia_tool::Error` via `types.rs` line 4) has 30 variants. Variant mapping is **lossy** for two cases:
  - `Stale { name, seen, current }` has no `synthia_core::Error` equivalent. The closest is `Error::Internal(...)` which collapses the three fields into one `String`. The 46 KB `ToolRegistry` uses `Stale` at `registry.rs` line 1089 (returns `Result<ToolOutput, ToolError>`) — folding it would lose the structured fields needed by `StaleOrUnknown`-aware callers.
  - `Cancelled` is a unit variant with no `synthia_core::Error` equivalent (closest is `Error::Internal("cancelled".to_string())`).

- ToolError call-sites that use non-folding-friendly variants:
  - `crates/synthia-core/src/tool/registry.rs:1089` — `Result<ToolOutput, ToolError>` (the merge target)
  - `crates/synthia-core/src/tool/plugin_registry.rs:625` — same return type
  - `crates/synthia-core/src/tool/registry.rs:541` — same
  - `crates/synthia-core/src/tool/plugin_registry.rs:546` — `ToolError` import

- The 3 other call-sites in `synthia-agent` (`AgentError::ToolError { tool, message }`) are an **unrelated** `AgentError` variant, not the descriptor's `ToolError`. They will need import-path updates only.

- Other matches (e.g., `crates/synthia-server/src/error.rs:24`, `crates/synthia-server/src/error.rs:59`) are `ServerError::ToolError(String)`, also unrelated.

**Conclusion:** Two of the eight variants (`Stale`, `Cancelled`) cannot map 1:1. The plan rule says "if any has no mapping → keep as `synthia_tool::ToolError`". The spec's open sub-decision is resolved.

**Downstream:** Task 17 (collision resolution) keeps `ToolError` in the moved `descriptor.rs`; `UnifiedToolAdapter::execute` returns `Result<ToolOutput, ToolError>`. Task 15 (`lib.rs` re-exports) includes `ToolError` in the `descriptor` re-export.

---

## 2. Descriptor location — **split into `descriptor.rs`**

**Decision:** Create new file `crates/synthia-tool/src/descriptor.rs` and re-export the 7-method `Tool` trait from there. Do not absorb into `traits.rs`.

**Evidence (LOC verified against disk):**

- `crates/synthia-core/src/tool/descriptor.rs` actual LOC: **245** (verified via `wc -l`).
- `crates/synthia-tool/src/traits.rs` actual LOC: **107** (verified via `wc -l`).

Drop 4 items from `descriptor.rs`:

| Item | Lines | LOC dropped |
|---|---|---|
| `pub struct ToolOutput` + `impl ToolOutput` | 22–41 | 20 |
| `pub trait Tool: ... { ... }` (3-method) | 101–119 | 19 |
| `pub enum ToolCategory { ... }` | 192–205 | 14 |
| `pub enum ExecutionMode { ... }` | 207–217 | 11 |
| **Total** |  | **64** |

Remaining `descriptor.rs` after drops: 245 − 64 = **181 LOC** (10 absorbed types: `ToolInput`, `ToolMetadata`, `ToolError`, `ToolContext`, `ToolDescriptor`, `ToolExample`, `ToolProvenance`, `ContextSource`, `ToolExposure`, `CancelBehavior`).

Merged total: 181 (descriptor post-drops) + 107 (current `traits.rs`) = **288 LOC**.

**Threshold:** plan rule says ≤ 250 LOC to keep in `traits.rs`. 288 > 250 → **must split**.

**Downstream:** Task 7 (move `descriptor.rs`) keeps the file as a new top-level module. Task 15 (`lib.rs`) declares `pub mod descriptor;` and re-exports the 10 types. The 7-method `Tool` trait continues to live in `traits.rs`; `descriptor.rs` co-locates with its I/O shapes.

**Note (per spec):** Plan's File Manifest lists `capability.rs` separately. The spec (Section 5.1) also says `ToolCapabilities` + `CapabilityBroker` "absorb into `traits.rs` vs. standalone — see Section 11 #2". The plan resolves this as standalone `capability.rs` (51 LOC, well under 250). This is consistent with sub-decision #2 because `capability.rs` is a separate file from `descriptor.rs`.

---

## 3. `ToolRegistry` split — **split into `registry.rs` + `dispatch.rs`**

**Decision:** Split the merged `ToolRegistry` into two files in `crates/synthia-tool/src/registry/registration/`:
- `registry.rs` — `ToolRegistry` struct, `ProviderEntry`, `Materialization`, `ToolIdentity`, `RegistrationToken`, `RegistrationScope`, `ToolGeneration`, `RegistrationError`, `StaleOrUnknown`, `new()`, `default()`, `with_max_concurrent()`, `register_provider()`, `unregister()`, `materialize()`, `resolve()`, `resolve_now()`, `register_entry()`, `len()`, `is_empty()`, `contains()`, `Clone` impl.
- `dispatch.rs` — `run_with_context()`, `execute_tools()`, `metadata_snapshots()` (renamed from 9 KB's `snapshot()`).

**Evidence (LOC verified against disk):**

- `crates/synthia-core/src/tool/registry.rs` actual LOC: **1406** (verified via `wc -l`).
- `crates/synthia-tool/src/registry/registration/registry.rs` actual LOC: **279** (verified via `wc -l`).
- Combined: **1685 LOC**. The two `ToolRegistry` struct definitions merge into one (subtract ~15 LOC for the duplicate struct + Default/Clone impls). Net merged LOC ≈ **1670**.

**Threshold:** plan rule says ≤ 350 LOC to keep in one file. 1670 >> 350 → **must split**.

**Downstream:** Task 8 (merge) creates both files. Task 15 (`registry/registration/mod.rs`) declares `pub(super) mod dispatch;`. No re-export changes needed (the public surface is `ToolRegistry`; method location is internal).

---

## 4. `ToolEvent` location — **delete; `on_tool_event` becomes a no-op**

**Decision:** Delete the `ToolEvent` enum. Remove the `on_tool_event` method from `ToolProvider` (or keep as default no-op without the enum — the trait method already has a no-op default body, so we can drop the parameter type entirely).

**Evidence (counted via Grep `ToolEvent`):**

- `crates/synthia-core/src/tool/provider.rs:25` — `async fn on_tool_event(&self, _event: &ToolEvent) {}` (trait method definition with no-op default body).
- `crates/synthia-core/src/tool/provider.rs:30` — `pub enum ToolEvent { Registered, Unregistered, Reloaded }` (enum definition).
- **Zero** `ToolEvent::` construction call-sites across `crates/`.
- **Zero** `on_tool_event(` invocations across `crates/` (the method is never called by any implementor).
- The variant fields in `ToolEvent` (`Registered { name }`, `Unregistered { name }`, `Reloaded { name }`) overlap with the 46 KB `ToolRegistry`'s own registration bookkeeping (which already emits its own log spans at `registry.rs:231` with `"exception.type" = "ToolError"` as the only structured field — see also the `crates/synthia-tool/tests/tool_span.rs:317–318` test that asserts on `"ToolError"`).

**Threshold:** plan rule says ≥ 3 distinct construction call-sites to keep; else delete. We have **0** call-sites → **delete**.

**Downstream:** Task 6 (move `provider.rs`) drops the `ToolEvent` enum and the `on_tool_event` parameter type. `ToolProvider` becomes a 3-method trait: `id()`, `list_tools()`, `get_tool()`. The trait method `on_tool_event` either disappears (cleanest) or remains with a no-op body and no parameter type. Recommended: remove the method, since no implementor overrides it. Task 15 (`provider` re-export) drops `ToolEvent` from the re-export list.

**Note:** `FileChangeEvent` (24 matches across 6 files) is **unrelated** — it's a filesystem progress event, not a tool lifecycle event. It is kept.

---

## 5. `UnifiedToolAdapter` descriptor caching — **eager**

**Decision:** `UnifiedToolAdapter` stores `Arc<dyn Tool>` + cached `ToolDescriptor` (computed at `new` time). Matches the spec's Section 5.5 signature exactly: `pub fn new(inner: Arc<dyn Tool>, descriptor: ToolDescriptor) -> Self`.

**Evidence (verified by reading 8 of the 9 builtin tool files):**

The spec says "9 builtin tools" but the actual count is **8** Tool implementations (the 9th file `path.rs` is a helper module with `resolve_path` / `check_path_safety` functions, not a `Tool` impl).

| File | `name()` returns | `description()` returns | `parameters()` returns | `mode()` |
|---|---|---|---|---|
| `builtin/read.rs` | `"read"` (literal) | `"Reads the contents of files"` (literal) | `json!{...}` literal | default `Parallel` |
| `builtin/write.rs` | `"write"` (literal) | `"Creates or overwrites files"` (literal) | `json!{...}` literal | override → `Sequential` |
| `builtin/shell.rs` | `"shell"` (literal) | `"Execute a shell command..."` (literal) | `json!{...}` literal | override → `Sequential` |
| `builtin/glob.rs` | `"glob"` (literal) | `"Finds files based on pattern matching"` (literal) | `json!{...}` literal | default |
| `builtin/grep.rs:115-123` | literal | literal | `json!{...}` literal | (not overridden) |
| `builtin/multi_edit.rs:26-65` | literal | literal | `json!{...}` literal | override |
| `builtin/web.rs:66-74` | literal | literal | `json!{...}` literal | (not overridden) |
| `builtin/apply_patch/tool.rs:184-207` | literal | literal | `json!{...}` literal | override |

All 8 Tools expose `name()`, `description()`, `parameters()` as **literal-returning** methods (no I/O, no allocation beyond a `String` from `to_string()`). They cover 4 of 13 `ToolDescriptor` fields.

The remaining 9 fields (`category`, `provenance`, `cancel_behavior`, `examples`, `permission_required`, `prompt_visible_provenance`, `is_hidden`, `is_user_invocable`, `exposure`) have no accessor on the 7-method `Tool` trait. Adding them would break the trait (the 8 Tools would need to implement them, even with `..Default::default()` body).

**Two options:**

1. **Lazy (compute on first `descriptor()` call):** adapter stores only `Arc<dyn Tool>` + an `OnceCell<ToolDescriptor>`. Each tool would need either a new `fn descriptor(&self) -> ToolDescriptor` method on the trait (breaking change to all 8) or hard-coded descriptor knowledge inside the adapter per type (impossible — adapter is generic). Lazy is **not feasible** without trait changes.

2. **Eager (caller passes `ToolDescriptor` to `new`):** the registration path (e.g., the agent builder that creates builtin tool instances) constructs both the `Arc<dyn Tool>` and a `ToolDescriptor` value at the same time. For each of the 8 builtin tools, a small per-tool `pub fn descriptor() -> ToolDescriptor` helper (or `const fn`) lives next to the tool and returns the full `ToolDescriptor` literal. The adapter then takes both.

**Threshold:** plan rule says "if all 9 builtin tools already expose `description()` + `parameters()` + a category constant cheaply → lazy". The 8 actual Tools expose 3 of 13 fields cheaply, not all 13. They cannot build a `ToolDescriptor` from the trait surface alone. → **eager** (per the alternative clause in the plan rule).

**Downstream:** Task 16 (`UnifiedToolAdapter`) constructs with both `inner` and `descriptor` as the spec's Section 5.5 pseudocode already shows. Each of the 8 builtin tool files gains a small `pub fn descriptor() -> ToolDescriptor` constant (or `lazy_static` / `OnceLock`); the registration site calls both. The adapter stores `descriptor: ToolDescriptor` and exposes it via `pub fn descriptor(&self) -> &ToolDescriptor { &self.descriptor }`. No `OnceCell` / lazy machinery needed.

**Note on the "9 builtin tools" wording in the spec:** The spec counts `path.rs` as a Tool, but the file is a helper module. This decision record treats the actual count as 8. Task 8 and Task 16 should reflect 8 Tools (the spec text is a planning-time overcount, harmless to the eager-cache design).

---

## Summary

| # | Decision | Outcome |
|---|---|---|
| 1 | `ToolError` location | Keep as `synthia_tool::descriptor::ToolError` |
| 2 | Descriptor location | Split into `crates/synthia-tool/src/descriptor.rs` |
| 3 | `ToolRegistry` split | Split into `registry.rs` + `dispatch.rs` |
| 4 | `ToolEvent` location | Delete; drop `on_tool_event` from `ToolProvider` |
| 5 | `UnifiedToolAdapter` cache | Eager (`Arc<dyn Tool> + ToolDescriptor` stored) |

These decisions feed into Task 6 (move `provider.rs` — drop `ToolEvent`), Task 7 (move `descriptor.rs` — create new top-level module), Task 8 (merge `ToolRegistry` — split into `registry.rs` + `dispatch.rs`), Task 15 (`lib.rs` — re-exports), Task 16 (`UnifiedToolAdapter` — eager), and Task 17 (delete 3-method `Tool` + drop `Stale`/`Cancelled` in error mapping).
