# Retrospective: synthia-loop-agent-turn-realization (Change #2)

> **Date**: 2026-07-19
> **Scope**: Wire change #1 infrastructure (HookOutcome, LoopDetector, GoalService, ExtensionRegistry) into main_loop
> **Tasks completed**: 22 / 23 (Task 9.1 is this document)

---

## 1. What Worked

### 1.1 UnifiedHookDispatcher as single dispatch point

The `UnifiedHookDispatcher` pattern (D6) cleanly consolidated two previously separate dispatch paths (`HookRegistry` + `ExtensionRegistry`) into one entry point. The hook-first ordering with combined outcome resolution (`Deny > ForwardToMainAgent > Allow`) is intuitive and easy to reason about. The short-circuit on `Deny` avoids unnecessary extension work.

**Evidence**: 5 dispatch tests pass; existing main_loop tests pass unchanged after migration from `HookBuilder` to `UnifiedHookDispatcher::dispatch()`.

### 1.2 Layered LoopDetector approach

Keeping `LoopDetectorSet` as the hard floor (P6 distrust) and adding `synthia-hook::LoopDetector` as a soft vote through the Hook trait was the right call. No existing doom_loop tests needed modification, and the new Layer 2 denial works orthogonally.

**Evidence**: All existing `check_doom_loop` tests pass unchanged; new test confirms Layer 2 can deny when Layer 1 passes.

### 1.3 From<ExtensionOutcome> bridge (D1)

A single `impl From<ExtensionOutcome> for HookOutcome` in `synthia-extension-v2` avoided introducing a third enum or a cross-crate dependency. Extension callbacks naturally convert before returning, and the canonical type (`HookOutcome`) stays in `synthia-hook`.

**Evidence**: 3 conversion tests (Allow, Deny, ForwardToMainAgent) pass; no circular dependency between crates.

### 1.4 SteeringChannel reuse for ForwardToMainAgent

Reusing the existing `SteeringChannel` drain mechanism with a new `SteeringPriority::Forwarded` variant avoided creating a new queue infrastructure. The rate limit (5/turn) and priority ordering (below User, above System) prevent steering floods.

**Evidence**: Priority ordering test + rate-limit enforcement test pass.

### 1.5 Helper extraction from main_loop

Extracting `maybe_auto_trigger_self_reflect`, `maybe_auto_trigger_compact_context` to `iteration/auto_trigger.rs` and `emit_turn_event`/`handle_hook_outcome` to `run/helpers.rs` reduced main_loop by ~200 lines without behavioral changes.

---

## 2. What Didn't Work (or Fell Short)

### 2.1 main_loop line count target not met

**Target**: ≤ 850 lines. **Actual**: 1087 lines (down from 1285, a 15% reduction).

The `async_stream::stream!` macro fundamentally limits decomposition: `yield` must be a statement in the macro body, preventing extraction of the stream block itself. The 850-line target requires decomposing the `stream!` into a state-machine with explicit phases, which is a significantly larger refactor than this change's scope.

**Lesson**: Set line-count targets only after understanding macro-level constraints. The `stream!` block is an inherent complexity boundary that can't be broken through simple extraction.

### 2.2 Extension-only event coverage incomplete

The original design listed 19 Extension events, but only 10 are `HookEvent` variants. The remaining 9 (PreSteering, PostSteering, PreSubagentSpawn, PostSubagentSpawn, MCP, OAuth, DefinitionDrift, etc.) don't have `HookEvent` equivalents yet.

**Result**: `PreMessageDrop` was wired (it has a `HookEvent` variant), but the extension-only events are deferred to change #3/#4. This is a scope management trade-off, not a failure — adding new `HookEvent` variants would have expanded this change's surface area significantly.

### 2.3 Dual-Arc wrapping for ServiceRegistry

The `Arc<dyn Extension>` → `Arc<dyn Any + Send + Sync>` coercion doesn't work directly because `dyn Extension` is `!Sized`. The workaround requires dual-Arc wrapping (`Arc::new(Arc<dyn Extension>)`) and `Arc::downcast::<Arc<dyn Extension>>()` on retrieval.

**Lesson**: When `ServiceRegistry::register_with_capability` stores `Arc<dyn Any + Send + Sync>`, trait objects need a Sized inner wrapper. This pattern should be documented as a convention for future registrations.

### 2.4 GoalService admission gate is fire-and-forget

The design chose admission-only `GoalService::submit()` (fire-and-forget) over a `TaskGoalHandle` that main_loop polls. This means the goal service can admit a task but main_loop has no structured way to receive mid-execution goal state transitions (e.g., "goal re-prioritized by another agent").

**Impact**: The `goal_tracker` field provides progress tracking, but it's a separate trait and separate instance. The two services aren't unified.

---

## 3. Surprises

### 3.1 OpenSpec validator's SHALL/MUST extraction is line-sensitive

The OpenSpec validator's `extractRequirementText` function only checks the **first non-header, non-metadata, non-blank line** of a requirement block for SHALL/MUST keywords. A requirement that starts with `WHEN ...` (a scenario) fails validation even though the body contains SHALL later.

**Fix**: Added a summary sentence with SHALL as the first line before the WHEN/THEN scenario. This is fragile — future spec writers must know this undocumented constraint.

**Recommendation**: The validator should scan the entire requirement body for SHALL/MUST, not just the first line. File as a tooling improvement.

### 3.2 AgentHookAdapter needs `?Sized` bound

The `Hook` trait's `on_event(&self, ...)` receiver requires `?Sized` on the implementing type because `AgentHookAdapter` wraps `dyn AgentHook` (a trait object, which is `!Sized`). Without `?Sized`, the blanket `impl Hook for T` fails.

**Impact**: Minor — just a bound annotation — but it's easy to miss if you're not familiar with Rust's Sized inference rules for trait objects.

### 3.3 `cargo fix --allow-dirty` was necessary

After moving `emit_turn_event` and `handle_hook_outcome` to `helpers.rs`, several imports in `main_loop.rs` became unused (`EventStore`, `append_agent_event`, `execute_self_reflect_tool_call`). Running `cargo fix --lib -p synthia-agent --allow-dirty` was the fastest way to clean these up, but the `--allow-dirty` flag requires care to avoid unintended changes.

### 3.4 collapsible_if lint in async_stream context

Clippy's `collapsible_if` lint triggered on patterns like `if let Some(x) { if let Some(y) {` inside the `stream!` macro. Merging to `if let Some(x) && let Some(y) {` worked, but the `let ... && let ...` syntax requires Rust 2024 edition or nightly. This is fine for our toolchain but worth noting for compatibility.

---

## 4. Lessons for Change #3 / #4

### 4.1 Define HookEvent variants before wiring

When a change requires new lifecycle events, define the `HookEvent` variants **first** (in the hook crate), then wire them in main_loop. This change discovered that 9 extension-only events had no `HookEvent` representation, forcing deferral. Change #3 (tool business) will likely need `PreToolExecute` / `PostToolExecute` variants — define them upfront.

### 4.2 Set realistic line-count targets based on macro constraints

The `async_stream::stream!` macro creates an inherent minimum size for `main_loop.rs`. Before setting a line-count target, identify the irreducible core (the `stream!` block) and set the target relative to that. For change #3, consider:
- Target: reduce main_loop by extracting state-machine phases (if feasible)
- Alternative: accept main_loop as a coordinator and focus extraction on the helper modules

### 4.3 Feature-gated integration requires dual-path testing

The `unified-registry` feature flag means `BuilderSteps::new()` has two construction paths (with and without `LoopServices`). Tests must cover both paths. Change #3 should:
- Add a CI gate that runs tests with `--features unified-registry` AND without
- Consider graduating `unified-registry` to a default feature once stable

### 4.4 Dual-Arc pattern for ServiceRegistry registration

When registering trait objects with `ServiceRegistry::register_with_capability`, use the dual-Arc pattern:
```rust
let ext_arc: Arc<dyn Extension> = /* ... */;
let any_arc: Arc<dyn Any + Send + Sync> = Arc::new(ext_arc);
registry.register_with_capability::<Extension>(any_arc);
// Retrieval:
let recovered = any_arc.downcast::<Arc<dyn Extension>>().unwrap();
```

This should be documented in `synthia-service` or `synthia-extension-v2` crate docs.

### 4.5 OpenSpec requirement format: SHALL in first line

When writing OpenSpec requirement specs, the first substantive line (after headers, metadata, blanks) **must** contain SHALL or MUST. Scenarios (WHEN/THEN) should come after a summary sentence that establishes the normative language.

### 4.6 Deprecation markers need `#[allow(deprecated)]` audit

After adding `#[deprecated]` to `HookBuilder::fire_*`, existing call sites need `#[allow(deprecated)]` annotations. Change #4 should:
1. Audit all `#[allow(deprecated)]` sites
2. Remove the deprecated methods after the 6-month window
3. Verify no new call sites have been added

### 4.7 ForwardToMainAgent durability is still an open question

The current design uses transient `MpscSteeringChannel` for forwarded messages. If a sub-agent runs for minutes and the parent crashes, forwarded messages are lost. Change #4 should evaluate whether long-running sub-agents need durable forwarding via `SessionInputQueue`.

### 4.8 Incremental extraction > big-bang refactor

The most successful pattern in this change was incremental extraction: move one coherent piece at a time (`auto_trigger.rs`, `helpers.rs`), verify tests pass, then move the next. The least successful pattern was trying to hit an arbitrary line-count target without understanding the structural constraint (`stream!`).

**Principle**: Extract until the remaining code has a single coherent responsibility, then stop. Don't extract just to hit a number.
