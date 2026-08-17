# Brainstorm: synthia-loop-agent-turn-realization (Change #2)

> Raw capture of brainstorming output for change #2 — loop/agent/turn 真化.
> This document feeds into design.md; it is NOT copied there.

---

## Background

Change #1 (架构基础设施) delivered 8 infrastructure capabilities across 34 PRs and was archived on 2026-07-19. These capabilities (EventV2, Extension, ServiceRegistry, GoalService, Hook unification, Tool materialization, Output sanitizer, Custom event renderer) are **defined but not consumed** by the main loop.

The current `main_loop.rs` is 1077 lines with `run_with_steps` spanning L101-936. It uses the **old** hook system (`HookBuilder` → `HookRegistry::fire_*`) and has **zero** integration with change #1's new infrastructure. Meanwhile, `AgentRunConfig` has 11+ fields that are destructured and discarded (`_` prefixed) in `run_with_steps`.

### Key Integration Gaps (from exploration)

| Gap | Description |
|-----|-------------|
| G1 | Two incompatible `GoalService` traits: `synthia-goal-service::GoalService` (submit/cancel admission) vs `synthia-service::goal::GoalService` (current/set/status tracking). `LoopServices` has no goal field. |
| G2 | Extension trait (19 events) not wired into main_loop — only 4 old `fire_*` hook methods used. PreCompact/PostCompact/PreSteering/PostSteering/SubagentSpawn events have no trigger points. |
| G3 | `ForwardToMainAgent` outcome defined in both `HookOutcome` and `ExtensionOutcome` but **never consumed** anywhere in the codebase. Sub-agent returning this outcome silently drops. |
| G4 | `HookOutcome` vs `ExtensionOutcome` are structurally identical 3-state enums but defined independently in two crates with no type-level bridge. |
| G5 | Extension's 9 extra events (Steering/Subagent/DefinitionDrift/MCPRoute/OAuthFlow) have no Hook system counterpart. Main_loop lifecycle points only trigger Hook events. |
| G6 | `LoopServices` missing `goal: Arc<dyn GoalService>` and `extension_registry: Arc<ExtensionRegistry>` fields. The `#[cfg(unified-registry)]` block references `loop_services.goal` but it doesn't exist. |
| G7 | `SessionInputQueue` (persisted JSONL) and `MpscSteeringChannel` (in-memory) have overlapping responsibilities. ForwardToMainAgent's target queue is undefined. |
| G8 | `ExtensionRegistry::register()` doesn't actually register with `ServiceRegistry` despite the doc comment saying it does. |

---

## Decision Chain

### Q1: How to bridge HookOutcome ↔ ExtensionOutcome?

**Context**: Both crates define the same 3-state enum. main_loop needs to handle outcomes from both systems.

**Options**:

1. **Re-export pattern**: `synthia-extension-hook` re-exports `HookOutcome` from `synthia-hook` and uses it directly. Extension callbacks return `HookOutcome` instead of `ExtensionOutcome`.
   - ✅ Zero duplication, single type to match on
   - ❌ Creates `synthia-extension-hook → synthia-hook` dependency

2. **Bridge enum**: Define `UnifiedOutcome` in `synthia-agent` that converts from both.
   - ✅ No cross-crate dependency
   - ❌ Third enum to maintain, conversion boilerplate

3. **Into conversion**: `impl From<ExtensionOutcome> for HookOutcome` in `synthia-extension-hook`.
   - ✅ Ergonomic, no new types
   - ❌ Still two source types, but convertible

**Decision (D1)**: **Option 3** — `impl From<ExtensionOutcome> for HookOutcome`. This avoids adding a dependency while providing ergonomic conversion. The canonical type is `HookOutcome`; extension callbacks convert before returning.

### Q2: How to consume ForwardToMainAgent in main_loop?

**Context**: When a sub-agent's hook/extension returns `ForwardToMainAgent { hint }`, the message should be routed to the parent agent's input.

**Options**:

1. **Steering channel injection**: Convert `ForwardToMainAgent` into a `SteeringMessage` and inject into the parent's `SteeringChannel`.
   - ✅ Reuses existing infrastructure, preserves priority ordering
   - ❌ Steering messages are typically user-initiated; semantically different

2. **New ForwardQueue**: Dedicated `mpsc::Receiver<ForwardedMessage>` in `LoopServices`.
   - ✅ Clean separation of concerns
   - ❌ Another queue to drain, another field on LoopServices

3. **SessionInputQueue**: Write forwarded messages to the persisted queue.
   - ✅ Durable across restarts
   - ❌ Overkill for transient forwarding; write amplification

**Decision (D2)**: **Option 1** — inject into `SteeringChannel` with a special `SteeringPriority::Forwarded` level. This reuses the existing drain mechanism in `drain_steering()` and adds only one priority level. The `hint` field becomes the steering content.

### Q3: How to integrate LoopDetector (synthia-hook) with existing LoopDetectorSet (synthia-guardian)?

**Context**: Two independent loop detection systems. `LoopDetectorSet` has 5 layers (DoomLoop/GenericRepeat/PingPong/PollNoProgress/GlobalCircuit). `synthia-hook::LoopDetector` has similarity-based detection returning `HookOutcome`.

**Options**:

1. **Replace**: Remove `LoopDetectorSet`, use only `synthia-hook::LoopDetector`.
   - ❌ Loses 5 specialized detectors; regression risk

2. **Parallel**: Run both, take the stricter result.
   - ✅ Defense in depth
   - ❌ Double detection overhead, potential conflicting signals

3. **Layered**: `LoopDetectorSet` as hard floor (P6 distrust), `synthia-hook::LoopDetector` as soft hook layer (configurable, can Allow where LoopDetectorSet would Deny but not vice versa).
   - ✅ Preserves safety net while enabling customization
   - ✅ Matches P6 (distrust LLM) principle — hard floor can't be overridden

**Decision (D3)**: **Option 3** — Layered approach. `LoopDetectorSet` runs first in `check_doom_loop` (unchanged). `synthia-hook::LoopDetector` runs as part of the unified Hook dispatch in `fire_before_tool`. If LoopDetectorSet detects, it wins. If it passes, hook-based LoopDetector gets a vote.

### Q4: How to unify GoalService?

**Context**: `synthia-goal-service::GoalService` (submit/cancel admission control) vs `synthia-service::goal::GoalService` (current/set/status tracking). Different semantics.

**Options**:

1. **Merge traits**: Single `GoalService` with both admission + tracking methods.
   - ❌ Violates ISP (Interface Segregation); consumers only need one half

2. **Keep separate, bridge in LoopServices**: `LoopServices` holds both `Arc<dyn GoalService>` (admission) and `Arc<dyn goal::GoalService>` (tracking).
   - ✅ No trait changes, backward compatible
   - ❌ Two fields for related functionality

3. **GoalServiceExt**: Extend `synthia-goal-service::GoalService` with tracking methods via supertrait.
   - ✅ Single reference in LoopServices
   - ❌ Breaking change to `synthia-goal-service`

**Decision (D4)**: **Option 2** — Keep separate, bridge in LoopServices. Add both fields to `LoopServices`. The admission service is used at turn start (gate entry); the tracking service is used during execution (report progress). This matches the "lazy loading" principle (P7) — don't unify until there's clear evidence both are always needed together.

### Q5: Should we introduce synthia-pipeline to replace StreamBuilder?

**Context**: StreamBuilder is 1077 lines in main_loop alone, with 5 step types and complex state management. A pipeline crate could provide a cleaner abstraction.

**Options**:

1. **Replace with synthia-pipeline**: New crate with Pipeline/Stage/Signal types.
   - ❌ Massive refactor, high risk, no incremental path
   - ❌ `async_stream::stream!` yield semantics can't be extracted to helper functions (Rust generator limitation)

2. **Evolve in place**: Refactor main_loop incrementally — extract phases, reduce state, consume new infrastructure.
   - ✅ Incremental, revert-safe, each PR < 500 LOC
   - ✅ Can evaluate pipeline crate after main_loop is < 400 lines

3. **Skip**: Leave StreamBuilder as-is, only add integration calls.
   - ❌ Doesn't address 540-line maintainability problem

**Decision (D5)**: **Option 2** — Evolve in place. Each PR extracts a coherent piece (e.g., "wire GoalService into main_loop") without changing the StreamBuilder pattern. After the loop is < 400 lines and all integrations are done, we can evaluate a pipeline crate in change #4.

### Q6: How to wire Extension events into main_loop?

**Context**: Extension has 19 events, main_loop only triggers 4 (via old hook system). Need to trigger the remaining 15.

**Options**:

1. **Full fire_* methods**: Add 15 new `fire_*` methods to `HookBuilder`.
   - ❌ Bloats HookBuilder to 21 methods

2. **Unified dispatch**: Replace `HookBuilder` with `UnifiedHookDispatcher` that accepts `HookEvent` (hook) and `ExtensionEvent` (extension) through a single `dispatch()` method.
   - ✅ Single entry point, extensible
   - ❌ Needs careful ordering (hooks before extensions? interleaved?)

3. **Event bus**: Use `EventBus::emit()` from change #1, both hooks and extensions subscribe.
   - ✅ Decoupled, matches event-sourcing pattern
   - ❌ Async emit makes ordering non-deterministic; hooks need synchronous gate (Allow/Deny)

**Decision (D6)**: **Option 2** — `UnifiedHookDispatcher`. It dispatches `HookEvent` to both the old `HookRegistry` (via `AgentHookAdapter`) AND `ExtensionRegistry`. Hooks run first (gate decision), then extensions run (observation/mutation). The dispatcher returns the combined `HookOutcome` (Deny wins over Allow, ForwardToMainAgent wins over Allow but not Deny).

### Q7: How to handle the 11 discarded AgentRunConfig fields?

**Context**: `run_with_steps` destructures `AgentRunConfig` and discards 11 fields with `_` prefix. These are consumed at construction time or by sub-components, not the main loop directly.

**Options**:

1. **Move to LoopServices**: All 11 fields become `LoopServices` fields, resolved via `ServiceRegistry::bound_service::<T>()`.
   - ❌ Not all 11 need to be in LoopServices; some are one-shot construction params

2. **Selective promotion**: Only promote fields that main_loop actually needs at runtime (goal_service, extension_registry, steering_channel, hook_registry). Others stay in config for construction.
   - ✅ Minimal change, each promotion is independently testable
   - ✅ Matches "lazy loading" principle

3. **Builder pattern refactor**: `AgentRunConfig` becomes a builder that produces `LoopServices` + `BuilderSteps` in one shot.
   - ❌ Over-engineering for current needs

**Decision (D7)**: **Option 2** — Selective promotion. Fields promoted to `LoopServices`:
- `goal_service: Arc<dyn GoalService>` (admission gate)
- `goal_tracker: Arc<dyn goal::GoalService>` (progress tracking)
- `extension_registry: Arc<ExtensionRegistry>` (event dispatch)
- `hook_dispatcher: Arc<UnifiedHookDispatcher>` (replaces `HookBuilder`)
- `loop_detector: Arc<LoopDetector>` (similarity detection)

Fields that stay in config for construction only:
- `context_assembler`, `model_router`, `fork_policy`, `subagent_session_factory`
- `tool_orchestrator`, `approval_service`, `sandbox_manager`, `guardian_coordinator`, `extension_manager` (consumed by `StepToolExecute`)

---

## Design Trade-offs Summary

| Decision | Choice | Rationale |
|----------|--------|-----------|
| D1 | `From<ExtensionOutcome> for HookOutcome` | No new dep, ergonomic conversion |
| D2 | SteeringChannel injection | Reuse existing drain, minimal new code |
| D3 | Layered loop detection | Safety net + customization, matches P6 |
| D4 | Two GoalService fields in LoopServices | ISP compliance, backward compatible |
| D5 | Evolve StreamBuilder in place | Incremental, revert-safe, evaluate pipeline later |
| D6 | UnifiedHookDispatcher | Single dispatch, hook-first ordering, combined outcome |
| D7 | Selective field promotion | Lazy loading, minimal change, independently testable |

---

## Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| R1: Hook ordering change breaks existing behavior | High | Dual-run mode: old HookBuilder + new UnifiedHookDispatcher coexist for 1 PR, then old path is removed |
| R2: ForwardToMainAgent creates steering flood | Medium | Rate-limit forwarded messages (max 5 per turn) + SteeringPriority::Forwarded is below User |
| R3: Two LoopDetectors give conflicting signals | Low | Layered approach: LoopDetectorSet is hard floor, hook-LoopDetector is soft vote |
| R4: Extension registration doesn't sync with ServiceRegistry (G8) | Medium | Fix in PR: ExtensionRegistry::register() calls ServiceRegistry registration. Single PR. |
| R5: Deprecation window for old Hook system | Low | 6-month window per D3 from change #1. change #2 adds `#[deprecated]` to old fire_* methods but doesn't remove them. |
| R6: main_loop line count may increase before decreasing | Medium | Accept temporary increase during wiring PRs; each PR must include a "state after" line count in description |

---

## Out of Scope (Deferred to Change #3-#4)

- `ToolCapabilities` per-tool struct → change #3
- WASM sandbox for plugins → change #3
- `CapabilityBroker` migration → change #3
- gRPC bridge streaming → change #4
- MCP http+ws+OAuth transports → change #4
- `synthia-pipeline` crate evaluation → after change #2, if main_loop > 400 lines
- Removing deprecated `AgentHook` / `HookRunner` → 6 months after change #2
