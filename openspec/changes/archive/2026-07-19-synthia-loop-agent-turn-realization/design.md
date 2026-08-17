# Design: synthia-loop-agent-turn-realization (Change #2)

## Context

Change #1 delivered 8 infrastructure capabilities that are **defined but not consumed** by the main loop. The current `main_loop.rs` (1077 lines) uses the old `HookBuilder` → `HookRegistry::fire_*` system with only 4 hook points (before_llm, after_llm, before_tool, after_tool). Meanwhile:

- `HookOutcome::ForwardToMainAgent` is defined but never consumed (sub-agent messages silently dropped)
- `synthia-hook::LoopDetector` is defined but never integrated (main_loop uses only `synthia-guardian::LoopDetectorSet`)
- `GoalService` admission control is defined but `LoopServices` has no goal field
- `Extension` trait has 19 events but only 4 have trigger points (matching old hook API)
- `AgentRunConfig` has 11+ fields destructured and discarded in `run_with_steps`
- `HookOutcome` and `ExtensionOutcome` are structurally identical 3-state enums in separate crates

### Constraints from Change #1

- ServiceRegistry + OutputBoundService abstraction locked (no reverse deps from service → agent)
- `HookOutcome::ForwardToMainAgent` semantics locked (PR-4.1)
- `AgentEvent::Custom` variant locked (PR-7.1) — convertToLlm must project Custom events
- `ToolContext::tool_id: ToolId` locked (PR-5.4) — subagent governance reads tool_id
- `OutputBound` trait locked (PR-6.1) — change #3 uses this for AST transparency

---

## Goals / Non-Goals

### Goals

1. Wire `ForwardToMainAgent` outcome into main_loop so sub-agents can route messages to parents
2. Integrate `synthia-hook::LoopDetector` as a soft hook layer alongside the existing `LoopDetectorSet` hard floor
3. Add GoalService admission control to main_loop turn entry
4. Trigger all 19 Extension events from main_loop lifecycle points
5. Unify hook + extension dispatch through a single `UnifiedHookDispatcher`
6. Promote runtime-needed fields from `AgentRunConfig` to `LoopServices`
7. Fix `ExtensionRegistry::register()` to actually register with `ServiceRegistry`

### Non-Goals

- Replace `StreamBuilder` with `synthia-pipeline` crate (evaluate after main_loop < 400 lines)
- Remove deprecated `AgentHook` / `HookRunner` (6-month deprecation window, remove in change #4)
- Introduce `ToolCapabilities` per-tool struct (change #3)
- WASM sandbox for plugins (change #3)
- gRPC streaming / MCP transports (change #4)
- Refactor `LoopContext` (543 lines) — orthogonal to this change

---

## Decisions

### D1: HookOutcome ↔ ExtensionOutcome Bridge

**Choice**: `impl From<ExtensionOutcome> for HookOutcome` in `synthia-extension-hook`

**Why**: Avoids adding `synthia-hook` as a dependency of `synthia-extension-hook` while providing ergonomic conversion. The canonical type is `HookOutcome`; extension callbacks convert before returning. No new types needed.

**Alternatives considered**:
- Re-export pattern (synthia-extension-hook re-exports HookOutcome) → adds cross-crate dependency
- Bridge enum `UnifiedOutcome` in synthia-agent → third enum to maintain
- Shared enum in new crate → over-engineering for 2 types

### D2: ForwardToMainAgent Consumption Path

**Choice**: Inject into parent's `SteeringChannel` with `SteeringPriority::Forwarded`

**Why**: Reuses existing drain mechanism in `drain_steering()`. Adds only one priority level. The `hint` field becomes the steering message content. No new queue infrastructure needed.

**Alternatives considered**:
- New `ForwardQueue` (dedicated mpsc channel) → another queue to drain, another field
- `SessionInputQueue` (persisted JSONL) → write amplification for transient forwarding

**Rate limiting**: Max 5 forwarded messages per turn. `SteeringPriority::Forwarded` is below `SteeringPriority::User` so user messages always take precedence.

### D3: LoopDetector Integration — Layered Approach

**Choice**: `LoopDetectorSet` as hard floor (P6 distrust), `synthia-hook::LoopDetector` as soft vote

**Why**: Preserves the 5-layer safety net while enabling hook-based customization. If `LoopDetectorSet` detects, it wins (hard deny). If it passes, hook-LoopDetector gets a vote (can warn or deny). Matches the "distrust LLM" principle — hard floor can't be overridden by hook configuration.

**Integration point**: `LoopDetectorSet` runs in `check_doom_loop` (unchanged). `synthia-hook::LoopDetector` runs as part of `UnifiedHookDispatcher::dispatch(HookEvent::PreToolUse)`.

### D4: GoalService — Two Fields in LoopServices

**Choice**: `LoopServices` holds both `goal_service: Arc<dyn GoalService>` (admission) and `goal_tracker: Arc<dyn goal::GoalService>` (tracking)

**Why**: Two traits have different semantics (admission vs tracking). Merging them violates ISP. Adding both as separate fields is backward-compatible and allows independent evolution.

**Integration point**: Admission gate at turn start (before `IterationStarted` event). Tracking update after tool execution and LLM response.

### D5: Evolve StreamBuilder In Place

**Choice**: Incremental refactor of main_loop without introducing `synthia-pipeline` crate

**Why**: Each PR extracts a coherent piece. `async_stream::stream!` yield semantics prevent extracting the loop body to a helper function (Rust generator limitation). After main_loop < 400 lines and all integrations are done, we can evaluate a pipeline crate.

### D6: UnifiedHookDispatcher

**Choice**: Replace `HookBuilder` with `UnifiedHookDispatcher` that dispatches to both `HookRegistry` and `ExtensionRegistry`

**Why**: Single entry point for both hook and extension dispatch. Hooks run first (gate decision), then extensions (observation/mutation). Combined outcome: Deny wins over Allow, ForwardToMainAgent wins over Allow but not Deny.

**Dispatch order**:
```
1. HookRegistry dispatch (via AgentHookAdapter) → HookOutcome
2. If Deny → return Deny immediately (short-circuit)
3. ExtensionRegistry dispatch → ExtensionOutcome → From → HookOutcome
4. Merge outcomes (Deny > ForwardToMainAgent > Allow)
```

### D7: Selective Field Promotion

**Choice**: Promote 5 runtime-needed fields from `AgentRunConfig` to `LoopServices`; keep 6 construction-only fields in config

**Fields promoted to LoopServices**:
- `goal_service: Arc<dyn GoalService>` (admission gate)
- `goal_tracker: Arc<dyn goal::GoalService>` (progress tracking)
- `extension_registry: Arc<ExtensionRegistry>` (event dispatch)
- `hook_dispatcher: Arc<UnifiedHookDispatcher>` (replaces HookBuilder)
- `loop_detector: Arc<LoopDetector>` (similarity detection)

**Fields staying in config** (consumed by sub-components at construction):
- `context_assembler`, `model_router`, `fork_policy`, `subagent_session_factory`
- `tool_orchestrator`, `approval_service`, `sandbox_manager`, `guardian_coordinator`, `extension_manager`

---

## Risks / Trade-offs

| Risk | Severity | Mitigation |
|------|----------|------------|
| R1: Hook dispatch order change breaks existing behavior | High | Dual-run mode: old `HookBuilder` + new `UnifiedHookDispatcher` coexist for 1 PR. Verify same outcomes on both paths before removing old. |
| R2: ForwardToMainAgent creates steering flood | Medium | Rate-limit: max 5 forwarded messages per turn. `SteeringPriority::Forwarded` < `User` priority. |
| R3: Two LoopDetectors give conflicting signals | Low | Layered: `LoopDetectorSet` is hard floor (can't be overridden). Hook-LoopDetector is soft vote. |
| R4: ExtensionRegistry double-registration not implemented (G8) | Medium | Single PR: add `ServiceRegistry::register_with_capability::<Extension>()` call inside `ExtensionRegistry::register()`. |
| R5: Deprecation window for old Hook system | Low | 6-month window. `#[deprecated]` on `HookBuilder::fire_*` in this change. Removal in change #4. |
| R6: main_loop line count may increase before decreasing | Medium | Accept temporary increase during wiring PRs. Each PR includes "state after" line count. Target: < 850 lines by end of change. |

---

## Migration Plan

1. **Phase 1 (PRs 1-3)**: Infrastructure — `From<ExtensionOutcome>` bridge, `UnifiedHookDispatcher` struct, `SteeringPriority::Forwarded`
2. **Phase 2 (PRs 4-6)**: LoopServices — promote 5 fields, wire `GoalService` admission, wire `ExtensionRegistry` dispatch
3. **Phase 3 (PRs 7-9)**: Main_loop — replace `HookBuilder` calls with `UnifiedHookDispatcher::dispatch()`, wire LoopDetector, wire ForwardToMainAgent, trigger all 19 Extension events
4. **Phase 4 (PRs 10-11)**: Cleanup — deprecate `HookBuilder::fire_*`, fix `ExtensionRegistry` double-registration, verify line count < 850

**Rollback**: Each PR is independently revert-safe. If `UnifiedHookDispatcher` causes issues, the old `HookBuilder` path remains functional behind the `unified-registry` feature flag.

---

## Open Questions

1. Should `ForwardToMainAgent` messages be persisted in `SessionInputQueue` (durable) or only in `MpscSteeringChannel` (transient)? Current design: transient only. May need durability for long-running sub-agents.
2. Should `GoalService::submit()` return a `TaskGoalHandle` that main_loop polls for state transitions, or should it be fire-and-forget admission? Current design: admission gate only (submit → check admitted → proceed or wait).
3. Should `ExtensionOutcome::ForwardToMainAgent` be rate-limited per-extension or globally? Current design: global rate limit (5/turn).
