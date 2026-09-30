# Brainstorm: synthia-tool-orchestrator-permission (Change #3)

> Raw capture of brainstorming output for change #3 — tool/orchestrator/permission business logic.

---

## Background

Change #1 delivered tool materialization identity (ToolId, ProviderId, Materialization, ScopeRef, ToolProvenance, OutputBound). Change #2 wires infrastructure into main_loop. Change #3 focuses on the **tool business logic** — connecting materialization to the orchestrator, replacing hardcoded permission checks with category-based checks, and integrating ToolCapabilities + CapabilityBroker.

### Key Integration Gaps (from exploration)

| Gap | Description |
|-----|-------------|
| G1 | `ToolCapabilities` + `CapabilityBroker` defined in `synthia-core` but **not connected** to `ToolExecutionContext` or `DefaultToolOrchestrator` |
| G2 | `PermissionChecker::security_check()` uses hardcoded tool-name strings (`"read_file"\|"write_file"\|"bash"`) instead of `ToolCategory` |
| G3 | `ToolPermission` sub-trait (synthia-tool) and `PermissionChecker` (synthia-permission) are parallel, unbridged systems |
| G4 | No WASM sandbox variant — `SandboxAttempt` has `None/Bubblewrap/Landlock/Seccomp` but no WASM; plugin tools have no isolation |
| G5 | `Materialization` (ToolId, ProviderId, Provenance) not connected to `DefaultToolOrchestrator` or `ToolCallRequest` |
| G6 | `OutputBound` trait defined but not called by orchestrator or `execute_and_emit` Phase 4 |
| G7 | `ToolCallRequest.permission` is a single `Permission` enum; orchestrator doesn't do dynamic permission upgrade/downgrade based on Provenance/Capability |

---

## Decision Chain

### Q1: How to connect ToolCapabilities to the tool execution pipeline?

**Options**:

1. **Add to ToolExecutionContext**: Add `capabilities: ToolCapabilities` field to `synthia-tool::ToolExecutionContext`.
   - ✅ Minimal change, backward compatible (Option<T>)
   - ❌ Legacy context type; new `ToolContext` in synthia-core already has it

2. **Migrate to ToolContext (synthia-core)**: Replace `ToolExecutionContext` usage with `synthia-core::ToolContext` which already has `capabilities`.
   - ✅ Right long-term direction
   - ❌ Large migration; every tool call site changes

3. **Bridge pattern**: `ToolExecutionContext::from_core_context(ctx: &ToolContext)` constructor that copies capabilities.
   - ✅ Incremental, no breaking changes
   - ❌ Data duplication

**Decision (D1)**: **Option 1** — Add `capabilities: Option<ToolCapabilities>` to `ToolExecutionContext`. The `ToolAdapter` populates it from `synthia-core::ToolContext` when the `unified-registry` feature is enabled. This is incremental and doesn't require migrating all tools at once.

### Q2: Replace hardcoded tool-name permission checks with ToolCategory?

**Options**:

1. **ToolCategory-based**: `security_check()` uses `ToolCategory` instead of tool-name strings.
   - ✅ Extensible — new tools declare their category, permission checks work automatically
   - ❌ Requires every tool to implement `ToolDefinition::category()` (some don't yet)

2. **Hybrid**: Category first, name fallback. If category is available, use it; otherwise fall back to name matching.
   - ✅ Backward compatible
   - ✅ Incremental — tools can be migrated category-by-category

3. **AST-based**: Parse tool arguments (bash commands, file paths) using tree-sitter for precise permission decisions.
   - ✅ Most precise (can distinguish `ls` from `rm -rf /` in bash)
   - ❌ Heavy dependency; tree-sitter grammars are large; not all tools have parseable args

**Decision (D2)**: **Option 2** — Hybrid category + name fallback. `security_check()` first checks `ToolCategory`; if unavailable, falls back to name matching. This is backward-compatible and extensible. AST-based checking is deferred to a future change (tree-sitter dependency is too heavy for change #3).

### Q3: How to bridge ToolPermission sub-trait and PermissionChecker?

**Options**:

1. **Deprecate ToolPermission**: Mark `ToolPermission` sub-trait as `#[deprecated]`, route all checks through `PermissionChecker`.
   - ✅ Single source of truth
   - ❌ Breaking for tools that implement `ToolPermission`

2. **Bridge implementation**: `PermissionChecker` implements `ToolPermission` (or vice versa).
   - ✅ Interoperable
   - ❌ Confusing — which one is authoritative?

3. **Keep separate, document relationship**: `ToolPermission` is the tool's self-declaration; `PermissionChecker` is the runtime policy. Both run, `PermissionChecker` wins on conflict.
   - ✅ Clear separation of concerns
   - ❌ Two systems to maintain

**Decision (D3)**: **Option 1** — Deprecate `ToolPermission` sub-trait with `#[deprecated]`. Route all permission checks through `PermissionChecker`. The `PermissionAlwaysAllow`/`PermissionAlwaysDeny` implementations can be replaced by `PermissionRule` entries in `MergedPolicy`. 6-month deprecation window.

### Q4: Materialization → Orchestrator connection

**Options**:

1. **ToolCallRequest gains ToolId**: Add `tool_id: Option<ToolId>` to `ToolCallRequest`. Orchestrator passes it through to events and results.
   - ✅ Minimal change, audit-friendly
   - ❌ `Option<ToolId>` means some calls won't have it

2. **Materialization on ToolCallResult**: Add `materialization: Option<Materialization>` to `ToolCallResult`.
   - ✅ Complete identity in result
   - ❌ Larger struct change

3. **Both**: `ToolCallRequest` gets `tool_id`, `ToolCallResult` gets `materialization`.
   - ✅ Full traceability
   - ❌ More fields to carry

**Decision (D4)**: **Option 1** — Add `tool_id: Option<ToolId>` to `ToolCallRequest`. The orchestrator resolves the tool, and if the registry returns a `Materialization`, the `tool_id` is populated. `ToolCallResult` gains a `tool_id` field that echoes the request's `tool_id`. Full `Materialization` on result is deferred — `tool_id` is sufficient for audit traceability.

### Q5: OutputBound integration

**Options**:

1. **Orchestrator calls OutputBound::bind()**: Before returning `ToolCallResult`, orchestrator applies output bounding.
   - ✅ Centralized, consistent
   - ❌ Orchestrator needs `OutputBound` dependency

2. **execute_and_emit Phase 4 calls OutputBound::bind()**: Replace `truncate_output` with `OutputBound::bind()`.
   - ✅ Minimal orchestrator change
   - ❌ Only applies in main_loop path, not standalone orchestrator usage

3. **Both**: Orchestrator provides the bound config; execute_and_emit applies it.
   - ✅ Orchestrator owns the config, main_loop applies it
   - ❌ Two points of application

**Decision (D5)**: **Option 2** — `execute_and_emit` Phase 4 calls `OutputBound::bind()` instead of `truncate_output`. The `OutputBound` instance comes from `LoopServices`. This is minimal change and the main_loop is the only place where output truncation should happen (standalone orchestrator usage is testing-only and doesn't need truncation).

### Q6: Dynamic permission based on Provenance/Capability

**Options**:

1. **Provenance-based upgrade**: `Provenance::Plugin` automatically upgrades to `RequireConfirm`; `Provenance::Ephemeral` upgrades to `RequireExplicit`.
   - ✅ Security by default for untrusted sources
   - ❌ May be too restrictive for trusted plugins

2. **Capability-based gate**: `CapabilityBroker::allowed("command_invoke") == false` → Block shell tools.
   - ✅ Fine-grained control
   - ❌ Requires capabilities to be populated correctly

3. **Combined**: Provenance sets the floor; capabilities can upgrade (but not downgrade) permission level.
   - ✅ Defense in depth
   - ✅ Provenance = trust level, capabilities = permission scope

**Decision (D6)**: **Option 3** — Combined approach. `ToolProvenance` sets the minimum permission level (Builtin ≤ AutoApprove, Plugin ≤ RequireConfirm, Ephemeral ≤ RequireExplicit). `ToolCapabilities` can upgrade within that bound (e.g., Plugin without `command_invoke` can be AutoApproved for filesystem tools but still RequireConfirm for shell tools). This matches P6 (distrust LLM) and P7 (lazy loading).

### Q7: WASM sandbox for plugins

**Options**:

1. **Add `SandboxAttempt::Wasm` now**: Full WASM runtime integration.
   - ❌ Major dependency (wasmtime/wasmer), large LOC, risky

2. **Stub `SandboxAttempt::Wasm`**: Add the variant but no runtime implementation. Return `ToolOutput::Error("WASM sandbox not yet implemented")`.
   - ✅ Type-level preparation, no runtime dependency
   - ❌ Not actually usable

3. **Defer**: Don't add the variant until there's a concrete WASM runtime integration plan.
   - ✅ No dead code
   - ❌ Type changes in a later change may require updating all match arms

**Decision (D7)**: **Option 2** — Stub `SandboxAttempt::Wasm { runtime: String }` variant. The type is prepared but the runtime is not. This enables code to handle the variant in match arms (returning a clear error) without a heavy dependency. When a WASM runtime is integrated (future change), only the execution path changes, not the type.

---

## Design Trade-offs Summary

| Decision | Choice | Rationale |
|----------|--------|-----------|
| D1 | Add capabilities to ToolExecutionContext | Incremental, backward compatible |
| D2 | Hybrid category + name fallback | Backward compatible, extensible |
| D3 | Deprecate ToolPermission sub-trait | Single source of truth (PermissionChecker) |
| D4 | ToolId on ToolCallRequest + ToolCallResult | Audit traceability, minimal struct change |
| D5 | OutputBound in execute_and_emit Phase 4 | Minimal orchestrator change |
| D6 | Combined Provenance + Capability | Defense in depth, matches P6+P7 |
| D7 | Stub SandboxAttempt::Wasm | Type preparation without runtime dep |

---

## Out of Scope (Deferred)

- WASM runtime integration (wasmtime/wasmer) — future change
- AST-based permission (tree-sitter) — too heavy for change #3
- Full `Materialization` on `ToolCallResult` — `tool_id` sufficient for now
- Migrate all tools from `ToolExecutionContext` to `synthia-core::ToolContext` — gradual over 6 months
