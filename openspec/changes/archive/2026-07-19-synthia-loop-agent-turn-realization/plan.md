# Implementation Plan: synthia-loop-agent-turn-realization

## Overview

**Change**: synthia-loop-agent-turn-realization (Change #2)
**Goal**: Wire change #1 infrastructure (hooks, extensions, goal service, loop detection) into main_loop
**Total PRs**: 16 (across 9 groups)
**Estimated effort**: 3-4 sessions

## Phase 1: Infrastructure (PRs 1.1-1.3, 2.1) — Independent, parallelizable

These PRs create new types with no main_loop changes. Can be implemented in parallel.

| PR | Capability | Files | LOC est. |
|----|-----------|-------|----------|
| 1.1 | From<ExtensionOutcome> bridge | synthia-extension-hook/src/lib.rs, Cargo.toml | ~30 |
| 1.2 | UnifiedHookDispatcher | synthia-hook/src/dispatcher.rs (new) | ~200 |
| 1.3 | AgentHookAdapter update | synthia-hook/src/hook_trait.rs | ~80 |
| 2.1 | SteeringPriority::Forwarded | synthia-agent/src/steering.rs | ~40 |

**Dependencies**: 1.2 depends on 1.3 (needs AgentHookAdapter working); 1.1 and 2.1 are independent.

**Verification**: `cargo test -p synthia-hook` + `cargo test -p synthia-extension-hook` + `cargo test -p synthia-agent -- steering`

## Phase 2: LoopServices + GoalService (PRs 3.1, 4.1-4.3) — Sequential

These PRs add fields to LoopServices and wire GoalService. Sequential because 4.2-4.3 depend on 4.1.

| PR | Capability | Files | LOC est. |
|----|-----------|-------|----------|
| 3.1 | LoopDetector Hook impl | synthia-hook/src/loop_detector.rs | ~60 |
| 4.1 | LoopServices GoalService fields | types.rs, loop_services.rs, construct.rs | ~80 |
| 4.2 | Admission gate | main_loop.rs | ~80 |
| 4.3 | Goal tracking | main_loop.rs | ~40 |

**Verification**: `cargo test -p synthia-hook` + `cargo test -p synthia-agent -- goal`

## Phase 3: Main_loop wiring (PRs 2.2, 3.2, 5.2-5.3, 6.2) — Mostly sequential

These PRs modify main_loop.rs. Must be sequential to avoid merge conflicts.

| PR | Capability | Files | LOC est. |
|----|-----------|-------|----------|
| 2.2 | ForwardToMainAgent consumption | main_loop.rs | ~60 |
| 3.2 | Layered detection | loop_detect.rs, main_loop.rs | ~30 |
| 5.2 | Wire session/LLM/tool events | main_loop.rs | ~120 |
| 5.3 | Wire steering/subagent/drift events | main_loop.rs + steering.rs + execute.rs | ~80 |
| 6.2 | BuilderSteps uses UnifiedHookDispatcher | types.rs, construct.rs, main_loop.rs | ~100 |

**Verification**: `cargo test -p synthia-agent` after each PR

## Phase 4: Cleanup + quality (PRs 5.1, 6.1, 7.1, 8.1-8.4) — Can be parallelized

| PR | Capability | Files | LOC est. |
|----|-----------|-------|----------|
| 5.1 | ExtensionRegistry double-registration | registry.rs | ~40 |
| 6.1 | HookBuilder deprecation | hook_builder.rs | ~20 |
| 7.1 | Promote fields from AgentRunConfig | types.rs, construct.rs, main_loop.rs | ~60 |
| 8.1-8.4 | Quality gates | — | — |

**Verification**: `cargo clippy` + `cargo test` per-module + line count check

## Risk Mitigation

1. **R1 (Hook ordering change)**: Phase 3 PR 6.2 is the critical PR — it switches from HookBuilder to UnifiedHookDispatcher. Run existing test suite before and after; if any test fails, keep dual-path behind feature flag.

2. **R6 (Line count increase)**: Phase 3 may temporarily increase main_loop.rs from 1077 to ~1100 lines. Phase 4 PR 7.1 should bring it back down to ≤ 850 by extracting dispatch calls and removing `_`-prefixed destructuring.

3. **Rollback strategy**: Each PR is independently revert-safe. The `unified-registry` feature flag controls whether LoopServices is populated; if issues arise, disable the flag to fall back to old behavior.

## Execution Order

```
Session 1: PR 0.1 → 1.1 + 1.3 (parallel) → 1.2 → 2.1 + 3.1 (parallel)
Session 2: PR 4.1 → 4.2 → 4.3 → 2.2 → 3.2
Session 3: PR 5.2 → 5.3 → 5.1 + 6.1 (parallel) → 6.2 → 7.1
Session 4: PR 8.1-8.4 → 9.1
```
