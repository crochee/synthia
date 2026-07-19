## 1. doom-loop-early-exit (P1.1)

- [ ] 1.1 Add `LoopAction` enum in `crates/synthia-guardian/src/types.rs`: variants `Continue, Warn, Block, RequirePermission, HardBlock`. Derive `Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize`.
- [ ] 1.2 Add `pub fn new_continue_action() -> Option<LoopAction>` helper to convert `LoopStatus` → `Option<LoopAction>` (returns `None` for `Ok`, `Some(Warn)` for `Warning`, `Some(Block)` for `Detected` with no special handling).
- [ ] 1.3 Write unit test: `LoopAction::RequirePermission` serializes to `"RequirePermission"` and deserializes back to same value.
- [ ] 1.4 Write unit test: `(LoopStatus::Detected, Some(LoopAction::RequirePermission))` is distinct from `(LoopStatus::Detected, Some(LoopAction::HardBlock))`.
- [ ] 1.5 Modify `LoopDetectorSet::check_tool_call` in [loop_detector.rs:281-297](file:///home/crochee/workspace/synthia/crates/synthia-guardian/src/loop_detector.rs#L281-L297) to return `(LoopStatus, Option<LoopAction>)` instead of `LoopDetectionResult`. (temporary signature for Phase 1; will be further refined in Phase 3)
- [ ] 1.6 Update the existing 9 unit tests in `loop_detector.rs` to handle the new tuple return type (minimal change: just `.0` / `.1` access).
- [ ] 1.7 Run `cargo test -p synthia-guardian`; verify all 9 existing tests pass with new signature.
- [ ] 1.8 Commit: "feat(guardian): LoopAction enum + tuple return from check_tool_call"

## 2. loop-detector-unified (P2.1, P2.2)

- [ ] 2.1 Add `use std::collections::HashMap;` to `loop_detector.rs` (already imported elsewhere; verify).
- [ ] 2.2 Rewrite `GenericRepeatDetector` in [loop_detector.rs:45-100](file:///home/crochee/workspace/synthia/crates/synthia-guardian/src/loop_detector.rs#L45-L100) to use `HashMap<(u64, u64), u32>` for O(1) lookups. Add `warn_threshold: u32 = 2` and `block_threshold: u32 = 3` fields.
- [ ] 2.3 Add `record_outcome(tool_id: u64, args_hash: u64, success: bool)` method to `GenericRepeatDetector`: success decrements count, failure is no-op.
- [ ] 2.4 Add `hash_tool_args` public function in `loop_detector.rs` (move from agent's `stream_builder/loop_detection.rs:28-38`).
- [ ] 2.5 Update `LoopDetectorSet::check_tool_call` to internally call `hash_tool_args` and pass `(tool_id, args_hash)` to `GenericRepeatDetector`. External API unchanged for this phase.
- [ ] 2.6 Write unit test: `GenericRepeatDetector::check` with 1000 calls of the same `(tool, args)` completes in < 100µs (amortized O(1) verified).
- [ ] 2.7 Write unit test: `record_outcome(success=true)` decrements count, after 3 successful outcomes the count returns to 0 and the HashMap entry is removed.
- [ ] 2.8 Write unit test: `hash_tool_args("tool", "args") == hash_tool_args("tool", "args")` (deterministic).
- [ ] 2.9 Write unit test: `hash_tool_args("tool_a", "args") != hash_tool_args("tool_b", "args")` (different tool name → different tool_id).
- [ ] 2.10 Write unit test: `hash_tool_args("tool", "args_a") != hash_tool_args("tool", "args_b")` (different args → different args_hash).
- [ ] 2.11 Run `cargo test -p synthia-guardian`; verify all tests pass (9 existing + 5 new = 14+).
- [ ] 2.12 Commit: "perf(guardian): GenericRepeatDetector O(1) HashMap + public hash_tool_args"

## 3. doom-loop-early-exit (P1.1 continued - DoomLoopDetector)

- [ ] 3.1 Add `DoomLoopDetector` struct to `loop_detector.rs` (ported from agent's `stream_builder/loop_detection.rs:221-261`). Fields: `recent_calls: VecDeque<(String, String)>`, `window_size: usize = 3`.
- [ ] 3.2 Implement `DoomLoopDetector::check(&mut self, tool_name: &str, args_json: &str) -> LoopStatus`:
  - Push to `recent_calls`, trim to `window_size`
  - If `len < 3` → return `Ok`
  - Compare last 3 entries for equality
  - Return `Detected` if all 3 match, else `Ok`
- [ ] 3.3 Add `DoomLoopDetector::reset(&mut self)` method (clear `recent_calls`).
- [ ] 3.4 Write unit test: 3 identical `(tool, args)` calls → 3rd returns `Detected`, 1st and 2nd return `Ok`.
- [ ] 3.5 Write unit test: 2 identical + 1 different → 3rd returns `Ok`, window resets.
- [ ] 3.6 Write unit test: `reset()` clears state; subsequent identical calls do NOT immediately trigger.
- [ ] 3.7 Add `doom_loop: DoomLoopDetector` field to `LoopDetectorSet`.
- [ ] 3.8 Update `LoopDetectorSet::check_tool_call` to evaluate `doom_loop` first; on `Detected`, return `(LoopStatus::Detected, Some(LoopAction::RequirePermission))`.
- [ ] 3.9 Add `LoopDetectorSet::reset()` to also reset `doom_loop`.
- [ ] 3.10 Write integration test in `loop_detector.rs`: 3 identical tool calls → final check returns `Detected + RequirePermission`.
- [ ] 3.11 Run `cargo test -p synthia-guardian`; verify all tests pass (14+ existing + 4 new = 18+).
- [ ] 3.12 Commit: "feat(guardian): DoomLoopDetector with RequirePermission early-exit signal"

## 4. loop-detector-unified (P2.2 - 5-detector collection)

- [ ] 4.1 Migrate `PingPongDetector` from agent's `stream_builder/loop_detection.rs` to guardian. Note: PingPong was originally in guardian — verify the version is identical (it should be).
- [ ] 4.2 Add `ping_pong: PingPongDetector` field to `LoopDetectorSet`.
- [ ] 4.3 Update `LoopDetectorSet::check_tool_call` to evaluate `ping_pong` after `doom_loop` and `generic_repeat`; on `Detected`, return `(LoopStatus::Detected, Some(LoopAction::Block))`.
- [ ] 4.4 Migrate `PollNoProgressDetector` from agent's `stream_builder/loop_detection.rs` to guardian (or keep guardian's existing version — verify they're equivalent).
- [ ] 4.5 Add `check_poll_result(&mut self, result: &str) -> LoopStatus` method to `LoopDetectorSet` (delegates to `poll_no_progress`).
- [ ] 4.6 Write unit test for `PingPongDetector` migration: A-B-A-B → 4th returns `Detected`.
- [ ] 4.7 Write unit test for `PollNoProgressDetector` migration: 10 identical results → 10th returns `Detected`.
- [ ] 4.8 Add `GlobalCircuitDetector` to `LoopDetectorSet` (verify it accepts `iteration` argument via `check_tool_call`).
- [ ] 4.9 Update `LoopDetectorSet::check_tool_call` signature to `check(&mut self, tool_name: &str, args_json: &str, iteration: usize) -> (LoopStatus, Option<LoopAction>)`. (Note: this is a **breaking change** — to be done in Phase 3 of plan.md with type alias compat)
- [ ] 4.10 Write integration test for full 5-detector collection: simulate doom loop, generic repeat, ping-pong, poll no-progress, and global circuit; verify each triggers its expected `LoopAction`.
- [ ] 4.11 Run `cargo test -p synthia-guardian`; verify all tests pass (18+ existing + 4 new = 22+).
- [ ] 4.12 Commit: "feat(guardian): unified 5-detector LoopDetectorSet with hash-based API"

## 5. hash-tool-args-public (P3.1)

- [ ] 5.1 Verify `hash_tool_args` is `pub` in `loop_detector.rs` (added in 2.4).
- [ ] 5.2 Re-export `hash_tool_args` from `synthia_guardian::lib.rs` if not directly accessible.
- [ ] 5.3 Add `pub use loop_detector::hash_tool_args;` to `synthia_guardian/src/lib.rs`.
- [ ] 5.4 Write integration test in `synthia-guardian/tests/hash_public.rs`: `synthia_guardian::hash_tool_args("a", "b")` is callable and returns the expected tuple.
- [ ] 5.5 Run `cargo test -p synthia-guardian`; verify integration test passes.
- [ ] 5.6 Commit: "feat(guardian): re-export hash_tool_args as public API"

## 6. stream-builder-v2 (deletion of local impl)

- [ ] 6.1 Update `crates/synthia-agent/src/dependencies.rs:21` to import `LoopDetectorSet` from `synthia_guardian` instead of `crate::stream_builder::loop_detection`.
- [ ] 6.2 Update `crates/synthia-agent/src/stream_builder/builder.rs:29-33, 228, 431` to use the new API:
  - Replace `loop_detection::LoopDetectorSet` with `synthia_guardian::LoopDetectorSet`
  - Update `check(&tu.name, &input_json_str, ctx.iteration)` to handle the tuple return type
  - On `RequirePermission`, call `permission.ask(DoomLoop { tool: tu.name, args: tu.input })`
  - On `Block` / `HardBlock`, set `loop_detected = true` (current behavior)
  - On `Warn`, log a warning and continue
- [ ] 6.3 Delete `crates/synthia-agent/src/stream_builder/loop_detection.rs` (entire file).
- [ ] 6.4 Update `crates/synthia-agent/src/stream_builder/mod.rs` to remove the `mod loop_detection;` declaration.
- [ ] 6.5 Verify `cargo build -p synthia-agent` succeeds with no warnings about missing modules.
- [ ] 6.6 Run `cargo test -p synthia-agent`; verify all 476+ existing tests pass.
- [ ] 6.7 Commit: "refactor(agent): delete local LoopDetectorSet, use synthia_guardian"

## 7. doom-loop-early-exit (e2e test)

- [ ] 7.1 Add e2e test in `crates/synthia-e2e/src/scenarios/loop_detection.rs`:
  - Construct `MockProvider` that returns 3 identical `read_file` tool calls in sequence
  - Construct agent with `Permission` mock that records `ask` invocations
  - Run agent loop; verify `permission.ask` was called with the doom-loop category
  - Verify the 3rd tool call was NOT executed (loop broke after ask)
- [ ] 7.2 Run `cargo test -p synthia-e2e --test loop_detection`; verify the new e2e test passes.
- [ ] 7.3 Verify existing 5 e2e tests in `loop_detection.rs` still pass (no regression).
- [ ] 7.4 Commit: "test(e2e): doom loop triggers permission.ask early-exit"

## 8. Final verification and changelog

- [ ] 8.1 Run full workspace build: `cargo build --all-targets --all-features`; verify no errors.
- [ ] 8.2 Run full test suite: `cargo test --workspace`; verify all 1161+ existing tests pass + 22+ new tests.
- [ ] 8.3 Run clippy: `cargo clippy --all-targets --all-features --tests --all`; fix any new warnings.
- [ ] 8.4 Run `cargo +nightly fmt --all`.
- [ ] 8.5 Verify `grep -r "stream_builder::loop_detection" crates/` returns 0 results (orphan removed).
- [ ] 8.6 Verify `grep -r "agent::loop_detector" crates/` returns 0 results (orphan removed in previous change).
- [ ] 8.7 Add CHANGELOG entry: "LoopDetectorSet unified: doom loop now triggers permission.ask, all loop detection consolidated in synthia-guardian".
- [ ] 8.8 Update `openspec/specs/stream-builder-v2/spec.md` to reflect the deletion of `stream_builder/loop_detection.rs` and the new `LoopAction::RequirePermission` handling.
- [ ] 8.9 Final commit: "refactor(guardian): complete LoopDetectorSet unification + doom loop early-exit"
