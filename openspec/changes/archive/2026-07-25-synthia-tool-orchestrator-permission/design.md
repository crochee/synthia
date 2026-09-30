# Design: synthia-tool-orchestrator-permission (Change #3)

## Context

Change #1 delivered tool materialization types (ToolId, ProviderId, Materialization, ToolProvenance, OutputBound, ScopeRef). Change #2 wires infrastructure into main_loop. The tool execution pipeline still uses hardcoded tool-name matching for permissions, has no capability integration, and doesn't carry materialization identity through the orchestrator.

The current permission flow is: `Tool.requires_permission()` (bool) → `PermissionChecker::security_check()` (hardcoded name matching) → `MergedPolicy::evaluate()` (string pattern) → `ApprovalService` (interactive/terminal/headless). This flow has no awareness of `ToolCategory`, `ToolCapabilities`, or `ToolProvenance`.

### Constraints from Change #1-#2

- `ToolId(Uuid)` locked (PR-5.1) — must appear on request/result
- `ToolProvenance` locked (PR-5.3) — must drive permission floor
- `OutputBound` trait locked (PR-6.1) — must be called in execution pipeline
- `ToolCategory` enum locked (tool sub-traits) — must replace name-based security checks
- `UnifiedHookDispatcher` from change #2 — permission decisions flow through hooks

---

## Goals / Non-Goals

### Goals

1. Connect `ToolCapabilities` to the tool execution pipeline via `ToolExecutionContext`
2. Replace hardcoded tool-name security checks with `ToolCategory`-based checks (hybrid fallback)
3. Unify permission checking by deprecating `ToolPermission` sub-trait in favor of `PermissionChecker`
4. Add `ToolId` to `ToolCallRequest` and `ToolCallResult` for audit traceability
5. Call `OutputBound::bind()` in `execute_and_emit` Phase 4
6. Implement combined Provenance + Capability permission model
7. Prepare WASM sandbox type (stub variant)

### Non-Goals

- WASM runtime integration (wasmtime/wasmer) — future change
- AST-based permission (tree-sitter) — too heavy for this change
- Full `Materialization` on `ToolCallResult` — `tool_id` sufficient
- Migrate all tools from `ToolExecutionContext` to `synthia-core::ToolContext`
- Refactor `DefaultToolOrchestrator` execution pipeline structure

---

## Decisions

### D1: ToolCapabilities in ToolExecutionContext

**Choice**: Add `capabilities: Option<ToolCapabilities>` to `synthia-tool::ToolExecutionContext`

**Why**: Incremental, backward compatible. `ToolAdapter` populates it from `synthia-core::ToolContext` when `unified-registry` feature is enabled. No need to migrate all tools to the new `ToolContext` at once.

### D2: Hybrid Category + Name Fallback

**Choice**: `security_check()` first checks `ToolCategory`; if unavailable, falls back to name matching

**Why**: Backward compatible and extensible. New tools declaring `ToolCategory::Shell` automatically get shell-level security checks. Legacy tools without category fall back to the existing name matching. `PermissionRule.pattern` gains `category:Shell` prefix syntax.

### D3: Deprecate ToolPermission Sub-trait

**Choice**: `#[deprecated]` on `ToolPermission` sub-trait; route all checks through `PermissionChecker`

**Why**: Single source of truth. `PermissionAlwaysAllow`/`PermissionAlwaysDeny` are replaced by `PermissionRule` entries in `MergedPolicy`. 6-month deprecation window. The `ToolPermission::check()` method's `PermissionDecision` maps to `PermissionChecker`'s `Permission` enum.

### D4: ToolId on ToolCallRequest + ToolCallResult

**Choice**: Add `tool_id: Option<ToolId>` to both `ToolCallRequest` and `ToolCallResult`

**Why**: Minimal struct change for audit traceability. The orchestrator populates `tool_id` from the registry's materialization data. Results echo the request's `tool_id`. Full `Materialization` on result is deferred — `tool_id` is sufficient for now.

### D5: OutputBound in execute_and_emit Phase 4

**Choice**: Replace `truncate_output` call with `OutputBound::bind()` from `LoopServices`

**Why**: Minimal orchestrator change. The `OutputBound` instance comes from `LoopServices.output_bound`. Main_loop is the only place where output truncation should happen; standalone orchestrator usage is testing-only and doesn't need truncation.

### D6: Combined Provenance + Capability Permission

**Choice**: Provenance sets minimum permission level; capabilities can upgrade within bound

**Permission floor by provenance**:
| Provenance | Minimum Level |
|------------|---------------|
| `Builtin` | `AutoApprove` (trusted) |
| `Plugin { extension_id }` | `RequireConfirm` (untrusted until proven) |
| `Ephemeral { source_id }` | `RequireExplicit` (highly untrusted) |

**Capability upgrade**: Within the provenance floor, `CapabilityBroker::allowed(capability_name)` can relax the level. E.g., a Plugin with `memory_read: true` but `command_invoke: false` can be `AutoApproved` for memory tools but stays `RequireConfirm` for shell tools.

### D7: Stub SandboxAttempt::Wasm

**Choice**: Add `SandboxAttempt::Wasm { runtime: String }` variant; return `ToolOutput::Error("WASM sandbox not yet implemented")` at runtime

**Why**: Type-level preparation without heavy runtime dependency. When WASM runtime is integrated (future change), only the execution path changes, not the type. All existing match arms get a new arm that returns a clear error.

---

## Risks / Trade-offs

| Risk | Severity | Mitigation |
|------|----------|------------|
| R1: Category-based checks miss tools without ToolCategory | Medium | Hybrid fallback: name matching when category is `None` |
| R2: ToolPermission deprecation breaks custom tools | Low | 6-month window; `#[deprecated]` warning with migration guide |
| R3: Provenance floor too restrictive for trusted plugins | Medium | CapabilityBroker can upgrade within provenance floor; plugin can declare capabilities in manifest |
| R4: OutputBound::bind() changes truncation behavior | Medium | `DefaultOutputBound` matches existing 50KiB/2000 line caps; verify existing truncation tests pass |
| R5: ToolId Option on ToolCallRequest means some calls lack identity | Low | Orchestrator always populates when registry provides Materialization; None for programmatic/test calls |

---

## Migration Plan

1. **Phase 1 (PRs 1-3)**: Additive types — ToolId on request/result, ToolCapabilities on context, Wasm stub variant
2. **Phase 2 (PRs 4-5)**: Permission rewrite — category-based security_check, ToolPermission deprecation
3. **Phase 3 (PRs 6-7)**: Integration — OutputBound in Phase 4, Provenance + Capability permission model
4. **Phase 4 (PRs 8-9)**: Quality gates + retrospective

**Rollback**: All changes are additive. `ToolPermission` deprecation doesn't remove the trait. Category-based checks fall back to name matching. `SandboxAttempt::Wasm` is a no-op variant.
