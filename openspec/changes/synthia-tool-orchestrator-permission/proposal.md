# Proposal: synthia-tool-orchestrator-permission

> Change #3 — Tool/Orchestrator/Permission business logic: connect materialization, category-based permissions, capability integration

## Why

Change #1 delivered tool materialization identity (ToolId, ProviderId, Materialization, ToolProvenance, OutputBound) and Change #2 wires infrastructure into main_loop. However, the **tool business logic** remains disconnected:

- `ToolCapabilities` + `CapabilityBroker` are defined in `synthia-core` but **not connected** to `ToolExecutionContext` or `DefaultToolOrchestrator`
- `PermissionChecker::security_check()` uses hardcoded tool-name strings (`"read_file"|"write_file"|"bash"`) instead of `ToolCategory`
- `ToolPermission` sub-trait and `PermissionChecker` are parallel, unbridged systems
- `Materialization` (ToolId, Provenance) not connected to `ToolCallRequest` or orchestrator events
- `OutputBound` trait defined but never called in the execution pipeline
- No WASM sandbox variant for plugin isolation
- `ToolCallRequest.permission` is a flat enum with no dynamic provenance/capability-based decision

## What Changes

1. **ToolCapabilities in ToolExecutionContext** — Add `capabilities: Option<ToolCapabilities>` field; `ToolAdapter` populates it from `synthia-core::ToolContext` when `unified-registry` feature is enabled
2. **Category-based permission checks** — `PermissionChecker::security_check()` uses `ToolCategory` first, name-matching fallback; `PermissionRule.pattern` supports `category:Shell` syntax
3. **Deprecate ToolPermission sub-trait** — Route all permission checks through `PermissionChecker`; 6-month deprecation window
4. **ToolId on ToolCallRequest/Result** — Audit traceability: `tool_id: Option<ToolId>` on request and result; orchestrator populates from registry materialization
5. **OutputBound integration in execute_and_emit** — Phase 4 calls `OutputBound::bind()` from `LoopServices` instead of `truncate_output`
6. **Combined Provenance + Capability permission** — Provenance sets minimum permission level; capabilities can upgrade within bound
7. **Stub SandboxAttempt::Wasm** — Type-level preparation for WASM sandbox; returns clear error at runtime

## Capabilities

### New Capabilities

| Capability | Description |
|------------|-------------|
| `tool-capability-integration` | ToolCapabilities in ToolExecutionContext + CapabilityBroker gate in orchestrator |
| `category-based-permission` | ToolCategory-based security_check + PermissionRule category pattern + ToolPermission deprecation |
| `tool-id-audit-trail` | ToolId on ToolCallRequest + ToolCallResult + orchestrator events |
| `output-bound-integration` | OutputBound::bind() in execute_and_emit Phase 4 replacing truncate_output |
| `provenance-capability-permission` | Combined Provenance floor + Capability upgrade permission model |
| `wasm-sandbox-stub` | SandboxAttempt::Wasm variant stub |

## Impact

- **Code**: `ToolExecutionContext` (+1 field), `PermissionChecker` (security_check rewrite), `ToolCallRequest` (+1 field), `ToolCallResult` (+1 field), `SandboxAttempt` (+1 variant), `execute_and_emit` Phase 4 rewrite
- **API**: `ToolPermission` sub-trait `#[deprecated]`, `PermissionRule.pattern` gains `category:` prefix syntax, `SandboxAttempt::Wasm` new variant
- **Dependencies**: `synthia-tool-orchestrator` gains `synthia-tool-materialization` dep (for ToolId)
- **Backward compatibility**: ToolPermission deprecated with 6-month window; all other changes are additive
