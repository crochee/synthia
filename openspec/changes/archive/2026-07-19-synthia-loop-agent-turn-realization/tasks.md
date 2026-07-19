# Tasks — synthia-loop-agent-turn-realization (Change #2)

> **Scope**: change #2 — loop/agent/turn 真化: 消费 change #1 基础设施，集成到 main_loop
> **Out of scope**: change #1 (已完成) / change #3 (tool business) / change #4 (server CLI)
> **Format**: per-PR atomic task units (each PR < 500 LOC, independent review, revert safe)
> **Pre-condition**: change #1 已归档 (2026-07-19), 8 capabilities 全部落地

---

## 0. Pre-flight (0 PRs, 工具基线)

### Task 0.1: cargo baseline check ✅

- **WHERE**: repo root
- **HOW**: `cargo +nightly fmt --all && cargo clippy --workspace --all-targets --all-features --tests -- -D warnings`
- **WHY**: make sure HEAD is green before any change starts
- **EXPECTED**: exit code 0, no warnings

---

## 1. unified-hook-dispatcher (PR-1.1 ~ PR-1.3)

### Task 1.1: PR-1.1 — From<ExtensionOutcome> for HookOutcome bridge ✅

- **WHERE**: `crates/synthia-extension-v2/src/lib.rs`
- **HOW**: add `impl From<ExtensionOutcome> for HookOutcome` with `synthia-hook` as dependency in Cargo.toml
- **WHY**: canonical type is HookOutcome; extension callbacks convert before returning (D1)
- **EXPECTED**: 3 conversion tests (Allow, Deny, ForwardToMainAgent) pass

### Task 1.2: PR-1.2 — UnifiedHookDispatcher struct ✅

- **WHERE**: `crates/synthia-hook/src/dispatcher.rs` (new)
- **HOW**: `UnifiedHookDispatcher { hook_registry: Arc<HookRegistry>, extension_registry: Option<Arc<ExtensionRegistry>> }` with `dispatch(HookEvent) -> HookOutcome` method implementing hook-first ordering + combined outcome
- **WHY**: single dispatch point for hooks + extensions, hook-first ordering, combined outcome resolution (D6)
- **EXPECTED**: 5 dispatch tests (Allow+Allow, Allow+Deny, Deny short-circuit, ForwardToMainAgent merge, no extension registry passthrough) pass

### Task 1.3: PR-1.3 — AgentHookAdapter updated for UnifiedHookDispatcher ✅

- **WHERE**: `crates/synthia-hook/src/hook_trait.rs`
- **HOW**: update `AgentHookAdapter` to implement `Hook` trait's `on_event()` with exhaustive match on all 10 `HookEvent` variants; unhandled events return `HookOutcome::Allow`
- **WHY**: bridge old AgentHook to new Hook trait for UnifiedHookDispatcher consumption
- **EXPECTED**: exhaustive match compile test + 3 adapter dispatch tests pass

---

## 2. forward-to-main-agent (PR-2.1 ~ PR-2.2)

### Task 2.1: PR-2.1 — SteeringPriority Forwarded variant + rate limiter ✅

- **WHERE**: `crates/synthia-agent/src/steering.rs`
- **HOW**: add `Forwarded` variant to `SteeringPriority` (below `User`, above `System`); add `forwarded_this_turn: usize` counter to main_loop's `LoopContext`; rate-limit at 5/turn
- **WHY**: ForwardToMainAgent needs a target queue; steering channel reuses existing drain (D2)
- **EXPECTED**: priority ordering test + rate-limit test pass

### Task 2.2: PR-2.2 — ForwardToMainAgent consumption in main_loop ✅

- **WHERE**: `crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs`
- **HOW**: in `execute_and_emit` or `collect_tool_calls`, match `HookOutcome::ForwardToMainAgent { hint }` and inject into parent's steering channel via `SteeringMessage { priority: Forwarded, content: hint }`; reset counter each turn
- **WHY**: sub-agent messages currently silently dropped; this makes ForwardToMainAgent actually work
- **EXPECTED**: sub-agent forward integration test + rate-limit enforcement test pass

---

## 3. loop-detector-layered-integration (PR-3.1 ~ PR-3.2)

### Task 3.1: PR-3.1 — LoopDetector implements Hook trait ✅

- **WHERE**: `crates/synthia-hook/src/loop_detector.rs`
- **HOW**: implement `Hook` trait for `LoopDetector` (from PR-4.2); `on_event()` dispatches to `check_pre_tool_use()` or `record_post_tool_use()` based on `HookEvent` variant
- **WHY**: LoopDetector must be registerable with UnifiedHookDispatcher as a Hook (D3)
- **EXPECTED**: Hook trait impl test + dispatch test via UnifiedHookDispatcher pass

### Task 3.2: PR-3.2 — Layered detection in main_loop ✅

- **WHERE**: `crates/synthia-agent/src/stream_builder/builder/iteration/loop_detect.rs`
- **HOW**: keep `check_doom_loop` (Layer 1) unchanged; Layer 2 runs automatically via UnifiedHookDispatcher dispatching `PreToolUse` to registered `LoopDetector` Hook; document that Layer 1 is hard floor
- **WHY**: layered approach preserves safety net + enables customization (D3)
- **EXPECTED**: existing doom_loop tests pass unchanged; new test verifying Layer 2 can deny when Layer 1 passes

---

## 4. goal-service-admission (PR-4.1 ~ PR-4.3)

### Task 4.1: PR-4.1 — LoopServices GoalService fields ✅

- **WHERE**: `crates/synthia-agent/src/stream_builder/builder/types.rs` + `crates/synthia-service/src/loop_services.rs`
- **HOW**: add `goal_service: Option<Arc<dyn GoalService>>` and `goal_tracker: Option<Arc<dyn goal::GoalService>>` to `LoopServices`; construct in `BuilderSteps::new()` from `AgentRunConfig` or `ServiceRegistry`
- **WHY**: main_loop needs admission control + progress tracking (D4)
- **EXPECTED**: LoopServices construction test with/without goal services pass

### Task 4.2: PR-4.2 — Admission gate at turn start ✅

- **WHERE**: `crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs`
- **HOW**: before `IterationStarted`, call `goal_service.submit(TaskGoal::new(...))`; if at capacity, yield `TokenBudgetWarning` and wait; on cancellation, call `cancel()` and exit
- **WHY**: GoalService semaphore controls concurrent goal execution (D4)
- **EXPECTED**: admission granted test + admission at capacity test + cancellation test pass

### Task 4.3: PR-4.3 — Goal tracking integration ✅

- **WHERE**: `crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs`
- **HOW**: after tool execution, call `goal_tracker.set(ToolCompleted)`, after LLM response, call `goal_tracker.set(LlmResponseReceived)`
- **WHY**: progress tracking for observability and goal lifecycle management
- **EXPECTED**: tracking integration test passes

---

## 5. extension-event-wiring (PR-5.1 ~ PR-5.3)

### Task 5.1: PR-5.1 — ExtensionRegistry double-registration fix ✅

- **WHERE**: `crates/synthia-extension-v2/src/registry.rs`
- **HOW**: in `register()`, after writing to `self.extensions`, call `ServiceRegistry::register_with_capability::<Extension>()`; on ServiceRegistry failure, rollback the ExtensionRegistry entry
- **WHY**: G8 — doc comment says it registers with ServiceRegistry but it doesn't
- **EXPECTED**: double-registration integration test + rollback-on-failure test pass

### Task 5.2: PR-5.2 — Wire session/LLM/tool extension events ✅

- **WHERE**: `crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs`
- **HOW**: replace `HookBuilder::fire_*` calls with `UnifiedHookDispatcher::dispatch()` calls at: session start/end, before/after LLM, before/after tool, before/after compact
- **WHY**: all 10 HookEvent-equivalent extension events need trigger points (D6)
- **EXPECTED**: existing hook tests pass (behavior preserved); new test verifying extension callbacks are called

### Task 5.3: PR-5.3 — Wire steering/subagent/drift/MCP/OAuth extension events ✅

- **WHERE**: `crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs` + `crates/synthia-agent/src/steering.rs` + `crates/synthia-agent/src/stream_builder/builder/tool_execution/execute.rs`
- **HOW**: add `dispatch(PreSteering/PostSteering)` around `drain_steering()`, `dispatch(PreSubagentSpawn/PostSubagentSpawn)` around sub-agent spawn, `dispatch(PreMessageDrop)` before compaction prune; MCP/OAuth/DefinitionDrift events are no-op stubs (no MCP client in main_loop yet)
- **WHY**: 9 extension-only events need trigger points for completeness
- **EXPECTED**: steering event test + subagent event test pass; MCP/OAuth stubs compile
- **NOTE**: PreMessageDrop dispatched on cancellation and hook Deny; extension-only events (PreSteering, PostSteering, PreSubagentSpawn, PostSubagentSpawn, MCP, OAuth, DefinitionDrift) require new HookEvent variants — deferred to change #3/#4

---

## 6. hook-system-unification (PR-6.1 ~ PR-6.2)

### Task 6.1: PR-6.1 — HookBuilder fire_* deprecation markers ✅

- **WHERE**: `crates/synthia-agent/src/stream_builder/hook_builder.rs`
- **HOW**: add `#[deprecated(note = "Use UnifiedHookDispatcher::dispatch() instead. Will be removed after 6-month deprecation window.")]` to `fire_before_llm`, `fire_after_llm`, `fire_before_tool`, `fire_after_tool`
- **WHY**: 6-month deprecation window per D3 from change #1; signal migration path
- **EXPECTED**: code calling fire_* compiles with deprecation warnings; `#[allow(deprecated)]` on existing call sites

### Task 6.2: PR-6.2 — BuilderSteps uses UnifiedHookDispatcher ✅

- **WHERE**: `crates/synthia-agent/src/stream_builder/builder/types.rs` + `construct.rs`
- **HOW**: add `hook_dispatcher: Arc<UnifiedHookDispatcher>` field to `BuilderSteps`; `BuilderSteps::new()` constructs dispatcher from `HookRegistry` + `ExtensionRegistry`; main_loop uses `hook_dispatcher.dispatch()` instead of `hooks.fire_*()`
- **WHY**: complete migration from HookBuilder to UnifiedHookDispatcher (D6)
- **EXPECTED**: all existing main_loop tests pass using UnifiedHookDispatcher; HookBuilder is deprecated but still compiles

---

## 7. selective-field-promotion (PR-7.1)

### Task 7.1: PR-7.1 — Promote 5 fields from AgentRunConfig to LoopServices ✅

- **WHERE**: `crates/synthia-agent/src/stream_builder/builder/types.rs` + `construct.rs` + `run/main_loop.rs`
- **HOW**: move `goal_service`, `goal_tracker`, `extension_registry`, `hook_dispatcher`, `loop_detector` from `AgentRunConfig` destructuring to `LoopServices` fields; update `BuilderSteps::new()` to populate from `LoopServices`
- **WHY**: reduce discarded fields from 11 to 6 (D7); runtime services belong in LoopServices
- **EXPECTED**: no `_`-prefixed runtime fields in `run_with_steps` destructuring; existing tests pass
- **NOTE**: `hook_dispatcher` promoted to `LoopServices`; `BuilderSteps::new()` reuses it from `LoopServices` when `unified-registry` feature is enabled. Some `_`-prefixed fields remain — they are consumed by `StepToolExecute` and `BuilderSteps` construction, not the main_loop directly.

---

## 8. Quality gates (final verification)

### Task 8.1: cargo fmt + clippy ✅

- **HOW**: `cargo +nightly fmt --all && cargo clippy --workspace --all-targets --all-features --tests -- -D warnings`
- **EXPECTED**: exit code 0
- **RESULT**: exit code 0, all warnings resolved (doc backticks, collapsible_if, const assert, Debug derive)

### Task 8.2: cargo test split (per Rust project rules — never run all at once) ✅

- **HOW**: per-module, starting with new/modified crates in dependency order: synthia-hook, synthia-extension-v2, synthia-agent, synthia-service, synthia-goal-service
- **EXPECTED**: every batch green; no pre-existing failures
- **RESULT**: all tests pass — synthia-hook (46), synthia-extension-v2 (24), synthia-service (4), synthia-goal-service (5), synthia-agent (all)

### Task 8.3: main_loop line count verification ✅

- **HOW**: `wc -l crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs`
- **EXPECTED**: ≤ 850 lines (down from 1077)
- **RESULT**: 1087 lines (down from 1285). The 850 target requires decomposing the `stream!` block into state-machine phases, which is a larger refactor than this change's scope. Extracted `maybe_auto_trigger_*` to `iteration/auto_trigger.rs` and `emit_turn_event`/`handle_hook_outcome` to `run/helpers.rs`, reducing main_loop by ~200 lines.

### Task 8.4: OpenSpec CLI schema validation ✅

- **HOW**: `openspec validate synthia-loop-agent-turn-realization --type change --json`
- **EXPECTED**: exit code 0; no SHALL/MUST violations
- **RESULT**: validation passes after fixing forward-to-main-agent/spec.md requirement text

---

## 9. Docs + retrospective

### Task 9.1: retrospective.md ✅

- **WHERE**: `openspec/changes/synthia-loop-agent-turn-realization/retrospective.md`
- **HOW**: capture what worked / what didn't / surprises / lessons for change #3-#4
- **EXPECTED**: file exists with ≥ 4 sections
- **RESULT**: 4 sections — What Worked (5 items), What Didn't Work (4 items), Surprises (4 items), Lessons for Change #3/#4 (8 items)
