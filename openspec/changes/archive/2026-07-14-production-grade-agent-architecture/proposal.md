## Archived 2026-07-16

This change was archived as a no-op execution. All 5 capabilities (`tool-cancellation-propagation`, `async-permission-deferred`, `scoped-tool-registry`, `doom-loop-proactive-detection`, `smart-compaction-agent`) and 2 additional specs (`guardian-circuit-breaker`, `tool-interrupt-cleanup`) were captured as design intent only — 83 implementation tasks were deferred. Delta specs are preserved at `openspec/changes/archive/2026-07-14-production-grade-agent-architecture/specs/` untracked, in keeping with the `bc4bfcf chore: untrack OpenSpec change files` policy. Anyone revisiting any of these areas should treat the proposal below as design intent for a future change, not as in-flight work.

---

## Why

Synthia has a solid ReAct loop foundation but critical architectural gaps vs production-grade agents (OpenCode, Codex, pi-mono). The most critical is a **broken cancellation chain** where `ToolAdapter::execute()` discards the `_cancellation_token` parameter — tools cannot be reliably interrupted mid-execution, creating safety and reliability risks. Beyond this, blocking permission handling, global-only tool registry, reactive loop detection, and truncation-based compaction all lag behind production-grade patterns. Addressing these gaps now will make Synthia suitable for production workloads.

## What Changes

**Tool Cancellation Propagation**
- From: `ToolAdapter` ignores `_cancellation_token` (underscore prefix), tools cannot be cancelled
- To: Cancellation token propagated through `ToolOrchestrator → ToolAdapter → Tool::call_with_sandbox()`, tools check token at yield points
- Reason: Safety-critical — runaway tools must be stoppable
- Impact: Breaking — `Tool` trait signature changes

**Async Permission Deferred**
- From: Synchronous blocking `HeadlessApprovalService` — agent freezes while waiting for permission
- To: `PermissionFuture` with `ask()` returning immediately — agent continues processing, permission resolves async
- Reason: Production UX requires agent responsiveness during permission waits
- Impact: Non-breaking — new method added to trait, existing sync `check()` preserved

**Scoped Tool Registry**
- From: Global static `ToolRegistry` — no per-session cleanup, tools persist across sessions
- To: `ScopedToolRegistry` with `ScopeGuard` RAII cleanup — tools registered per-session auto-deregister on session end
- Reason: Multi-agent scenarios need isolated tool namespaces
- Impact: Non-breaking — new registry type, existing registry unchanged

**Proactive Doom-Loop Detection**
- From: `GuardianCircuitBreaker` only trips after damage (reactive) — denial counting
- To: `DoomLoopDetector` proactively detects 3 consecutive identical (tool, args) before circuit breaker, triggers permission prompt
- Reason: OpenCode semantics — catch actual repeated calls, not just denial patterns
- Impact: Non-breaking — new detector alongside existing Guardian

**Smart Compaction Agent**
- From: 4-tier truncation compression — older messages simply discarded, losing semantic context
- To: Two-phase (backward token selection + LLM summarization) — older content summarized via second model call, incremental chaining
- Reason: LLM benefits from coherent summaries over raw truncated context
- Impact: Non-breaking — replaces truncation algorithm within existing ContextAssembler

## Capabilities

### New Capabilities

- `tool-cancellation-propagation`: Guaranteed cancellation propagation from AgentRunConfig through ToolOrchestrator → ToolAdapter → Tool::call_with_sandbox() with cooperative yield points in long operations
- `async-permission-deferred`: Non-blocking async permission via PermissionFuture. Agent continues other work while waiting. Supports "once"/"always"/"deny" with persistence
- `scoped-tool-registry`: Token-based scoped tool registration with automatic cleanup via ScopeGuard RAII. Per-session tool namespaces with last-wins materialization
- `doom-loop-proactive-detection`: Sliding window of 3 consecutive identical (tool_name, args_hash) triggers permission prompt before Guardian circuit breaker trips
- `smart-compaction-agent`: Two-phase compaction: backward token selection (8K keep) + LLM summarization call (same model, no tools, 4K output cap). Incremental summary chaining

### Modified Capabilities

- `tool-interrupt-cleanup` (existing spec): Clarification — existing spec requires `fail_interrupted_tools()` but cancellation propagation to tool's `call_with_sandbox()` was implicit. This change makes it explicit.
- `guardian-circuit-breaker` (existing spec): No change — DoomLoopDetector complements rather than replaces the circuit breaker

## Impact

### Affected Crates

| Crate | Changes |
|-------|---------|
| `synthia-tool` | Add `CancellationToken` to trait, add yield points, new `scoped_registry.rs` |
| `synthia-tool-orchestrator` | Fix token propagation, add PermissionFuture awaiting |
| `synthia-permission` | New `PermissionFuture` type, async `ask()` method |
| `synthia-guardian` | New `doom_loop_detector.rs` alongside circuit breaker |
| `synthia-context` | Extend ContextAssembler with LLM summarization step |

### Breaking Changes

- **`Tool::call_with_sandbox()` signature**: Adds `&CancellationToken` parameter — all built-in and external tool implementations must update

### Dependencies

- No new external dependencies for core fixes
- May use existing `xxhash` or `ahash` for doom-loop signature hashing (already in crate)
