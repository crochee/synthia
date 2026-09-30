## Context

Synthia has a solid ReAct loop foundation but critical architectural gaps compared to production-grade AI agents (OpenCode, Codex, pi-mono). After comprehensive parallel analysis of all four codebases, five P0/P1 gaps were identified:

**P0 Critical:**
- **Tool cancellation chain is broken**: `ToolAdapter::execute()` discards `_cancellation_token` (underscore prefix = ignored). Tools cannot be interrupted mid-execution.

**P1 High:**
- **Blocking permission**: `HeadlessApprovalService` is synchronous blocking — agent cannot continue while waiting for permission decisions.
- **Global-only tool registry**: No scoped registration — tools cannot be registered per-session with automatic cleanup.
- **Reactive loop detection**: `GuardianCircuitBreaker` only trips after damage is done. OpenCode proactively detects at 3 identical calls.
- **Truncation compaction**: Simple token-based truncation loses semantic context. OpenCode uses LLM summarization.

## Goals / Non-Goals

**Goals:**
- Fix the broken cancellation chain for safe, interruptible tool execution
- Add async permission Deferred pattern for non-blocking UX
- Implement scoped tool registry with RAII cleanup
- Add proactive doom-loop detection before circuit breaker trips
- Implement smart compaction with LLM summarization

**Non-Goals:**
- Not adopting Effect-rs framework (too invasive for current scope)
- Not changing the core ReAct loop architecture
- Not implementing full event sourcing with aggregate sequences
- Not adding WebSocket transport resilience (separate concern)

## Decisions

### D1: Tool Cancellation - Direct Parameter Addition

- **選擇**: Add `CancellationToken` parameter directly to `Tool::call_with_sandbox()` trait method
- **理由**: Built-in tools are few. Direct parameter addition is cleaner than adapter pattern. The underscore prefix on `_cancellation_token` in `ToolAdapter` is the actual bug — it was never meant to be used.
- **已考慮 alternative**: Adapter pattern for backward compat — rejected because it adds complexity without benefit when there are few internal tool implementations.

### D2: Async Permission - Future-based API

- **選擇**: `PermissionService::ask()` returns `PermissionFuture` wrapping `tokio::sync::oneshot::Receiver`. Orchestrator awaits future while agent stream continues.
- **理由**: Matches the reactive spirit of the agent — the stream keeps emitting other events while waiting for permission. `HeadlessApprovalService::ask()` immediately resolves with Denied.
- **已考慮 alternative**: Blocking thread pool — rejected because it defeats the purpose (agent still blocked, just on different thread).

### D3: Doom-Loop Detection - Dual System with Guardian

- **選擇**: New `DoomLoopDetector` alongside existing `GuardianCircuitBreaker`, not replacing it.
- **理由**: Different detection mechanisms are complementary:
  - `DoomLoopDetector`: signature-based (tool name + args hash), proactive, detects actual repeated calls
  - `GuardianCircuitBreaker`: denial-count-based, reactive, detects permission-denial patterns
- **已考慮 alternative**: Replace Guardian's loop detection — rejected because Guardian handles a different failure mode.

### D4: Smart Compaction - Extend ContextAssembler

- **選擇**: Extend existing `synthia-context` ContextAssembler, replacing truncation with LLM summarization.
- **理由**: ContextAssembler already has token budgeting logic. Reuse the selection algorithm, only replace the "discard old messages" step with "summarize old messages via LLM."
- **已考慮 alternative**: New standalone `SmartCompactionAgent` — rejected because it would duplicate token budgeting logic.

### D5: Scoped Registry - Token-based with ScopeGuard RAII

- **選擇**: `ScopedToolRegistry::register_scoped(tools, token)` with `ScopeGuard` that auto-deregisters on drop.
- **理由**: Token (`Arc<()>`) as unique scope identity enables O(1) deregistration. RAII guard ensures cleanup even on panic.
- **已考慮 alternative**: Reference-counted with weak refs — rejected as over-engineered GC-like semantics.

## Risks / Trade-offs

**[Risk] Breaking Tool trait API** → Mitigation: All internal tools must be updated. Document as breaking change. External tool implementors will need to update signatures.

**[Risk] LLM summarization can fail** → Mitigation: Fall back to simple truncation on failure. Log error but don't propagate. One-shot recovery prevents infinite compaction loops.

**[Risk] Async permission introduces state complexity** → Mitigation: `PermissionFuture` is simple oneshot wrapper. Document that futures must be awaited or dropped to avoid sender leaks.

**[Trade-off] Direct token parameter vs fiber automatic cancellation** → The explicit parameter approach is more verbose but more visible. Users can see exactly where cancellation is checked. Accept the verbosity for clarity.

**[Trade-off] Dual doom-loop vs single system** → Having two loop detectors adds complexity. However, they serve different purposes. Document clearly which to use when.

## Migration Plan

### Phase 1: P0 Tool Cancellation (Week 1)
1. Add `CancellationToken` to `Tool::call_with_sandbox()` trait
2. Fix `ToolAdapter` to propagate token
3. Add yield points to built-in tools
4. Test with long-running operations

### Phase 2: P1 Permission (Week 2-3)
1. Add `PermissionFuture` type
2. Update `PermissionService::ask()` interface
3. Update `HeadlessApprovalService` and TUI service
4. Update `DefaultToolOrchestrator` to await future

### Phase 3: P1 Scoped Registry (Week 3-4)
1. Create `scoped_registry.rs` module
2. Add `ScopeGuard` with RAII cleanup
3. Integrate with session lifecycle

### Phase 4: P1 Doom-Loop (Week 4-5)
1. Create `doom_loop_detector.rs`
2. Add to Guardian alongside circuit breaker
3. Add config option for threshold

### Phase 5: P1 Smart Compaction (Week 5-6)
1. Extend ContextAssembler with summarization step
2. Add LLM call for summary generation
3. Add incremental summary chaining
4. One-shot recovery test

**Rollback Strategy:**
- P0: Revert trait signature and ToolAdapter — simple revert
- P1: Remove new types, revert permission to sync — new types are additive
- All phases: JSONL session format is append-only, no migration needed

## Open Questions

1. **Should `Tool::call_with_sandbox()` take `&CancellationToken` or clone?** Taking by reference avoids clone overhead but lifetime is tied to call. Taking by value clones per call but simpler. Decision: take by reference (`&CancellationToken`).

2. **Permission "always" persistence — file or DB?** Decision: DB (same as session persistence). File adds another I/O path.

3. **Compaction summary stored where?** Decision: Insert as `compaction` message type in session event log. Filtered out in future selections.

4. **Config keys for thresholds?** Decision: Add to `ContextConfig`: `compaction_buffer` (default 20,000), `keep_tokens` (default 8,000), `doom_loop_threshold` (default 3).
