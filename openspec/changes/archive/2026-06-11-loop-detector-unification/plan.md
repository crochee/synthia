# LoopDetectorSet Unification Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development
> to implement this plan task-by-task.

**Goal:** Unify Synthia's two `LoopDetectorSet` implementations into a single canonical type in `synthia-guardian`, add the doom-loop early-exit signal (opencode-aligned), and consolidate 8 detectors into 5. Delete the orphan `synthia-agent::stream_builder::loop_detection` module.

**Architecture:** Single canonical `LoopDetectorSet` in `synthia-guardian` with 5 internal detectors (all `pub(crate)`). Public API: `check(tool, args_json, iteration) -> (LoopStatus, Option<LoopAction>)`. No `LoopDetector` trait (still deferred per D3.1). `LoopAction::RequirePermission` is a signal, not a direct permission call.

**Tech Stack:** Rust 1.x, `std::collections::HashMap`, `ahash::AHasher`, `serde` (for `LoopAction` Serialize/Deserialize), existing `synthia_permission::Permission` for ask flow.

---

## Phase 1: Algorithm Core (lowest risk, non-breaking)

### Task 1: GenericRepeatDetector O(1) HashMap in synthia-guardian

**Files**:
- MODIFY: `crates/synthia-guardian/src/loop_detector.rs:45-100` (replace `call_hashes: Vec<u64>` with `HashMap<(u64, u64), u32>`)
- MODIFY: `crates/synthia-guardian/src/loop_detector.rs:281-297` (internal hash computation)

**Steps**:

- [ ] **Step 1.1:** Write failing test in `loop_detector.rs`:
  ```rust
  #[test]
  fn generic_repeat_uses_hashmap_o1() {
      let mut det = GenericRepeatDetector::new();
      // 1000 calls of same (tool, args) should not slow down
      let start = std::time::Instant::now();
      for _ in 0..1000 {
          det.check("tool_a", "same_args");
      }
      assert!(start.elapsed().as_millis() < 100, "should be O(1) per call");
  }
  ```

- [ ] **Step 1.2:** Modify `GenericRepeatDetector` struct:
  ```rust
  pub(crate) struct GenericRepeatDetector {
      counts: std::collections::HashMap<(u64, u64), u32>,
      warn_threshold: u32,
      block_threshold: u32,
  }

  impl GenericRepeatDetector {
      pub(crate) fn new() -> Self {
          Self {
              counts: std::collections::HashMap::new(),
              warn_threshold: 2,
              block_threshold: 3,
          }
      }
      // ... rest unchanged
  }
  ```

- [ ] **Step 1.3:** Add `pub fn hash_tool_args(tool_name: &str, args_json: &str) -> (u64, u64)` to `loop_detector.rs` (copied from agent's `stream_builder/loop_detection.rs:28-38`).

- [ ] **Step 1.4:** Update `LoopDetectorSet::check_tool_call` to call `hash_tool_args` internally:
  ```rust
  pub fn check_tool_call(&mut self, tool_name: &str, args: &str) -> LoopDetectionResult {
      let (tool_id, args_hash) = hash_tool_args(tool_name, args);
      // ... pass (tool_id, args_hash) to generic_repeat.check
  }
  ```

- [ ] **Step 1.5:** Run `cargo test -p synthia-guardian --lib loop_detector`; verify all tests pass.

- [ ] **Step 1.6:** Commit: `perf(guardian): GenericRepeatDetector O(1) HashMap + public hash_tool_args`

**Verify:** Existing 9 tests in `loop_detector.rs` continue to pass; new `generic_repeat_uses_hashmap_o1` passes.

---

### Task 2: DoomLoopDetector + LoopAction enum

**Files**:
- MODIFY: `crates/synthia-guardian/src/types.rs` (add `LoopAction` enum)
- MODIFY: `crates/synthia-guardian/src/loop_detector.rs` (add `DoomLoopDetector` struct)

**Steps**:

- [ ] **Step 2.1:** Add `LoopAction` to `types.rs`:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
  pub enum LoopAction {
      Continue,
      Warn,
      Block,
      RequirePermission,
      HardBlock,
  }
  ```

- [ ] **Step 2.2:** Add `LoopStatus` to `types.rs` (moved from `loop_detector.rs`):
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum LoopStatus { Ok, Warning, Detected }
  ```

- [ ] **Step 2.3:** Add `DoomLoopDetector` to `loop_detector.rs` (ported from agent):
  ```rust
  pub(crate) struct DoomLoopDetector {
      recent_calls: std::collections::VecDeque<(String, String)>,
      window_size: usize,
  }

  impl DoomLoopDetector {
      pub(crate) fn new() -> Self {
          Self {
              recent_calls: std::collections::VecDeque::with_capacity(3),
              window_size: 3,
          }
      }
      pub(crate) fn check(&mut self, tool_name: &str, args_json: &str) -> bool {
          self.recent_calls.push_back((tool_name.to_string(), args_json.to_string()));
          while self.recent_calls.len() > self.window_size {
              self.recent_calls.pop_front();
          }
          if self.recent_calls.len() < 3 { return false; }
          let last3: Vec<_> = self.recent_calls.iter().rev().take(3).collect();
          last3[0] == last3[1] && last3[1] == last3[2]
      }
      pub(crate) fn reset(&mut self) { self.recent_calls.clear(); }
  }
  ```

- [ ] **Step 2.4:** Write tests:
  ```rust
  #[test]
  fn doom_loop_triggers_on_three_identical() {
      let mut det = DoomLoopDetector::new();
      assert!(!det.check("tool", "{}"));
      assert!(!det.check("tool", "{}"));
      assert!(det.check("tool", "{}"));
  }

  #[test]
  fn doom_loop_resets_on_different_args() {
      let mut det = DoomLoopDetector::new();
      det.check("tool", "{}");
      det.check("tool", "{}");
      assert!(!det.check("tool", r#"{"k":1}"#));
  }
  ```

- [ ] **Step 2.5:** Run `cargo test -p synthia-guardian --lib loop_detector`; verify tests pass.

- [ ] **Step 2.6:** Commit: `feat(guardian): DoomLoopDetector + LoopAction enum`

**Verify:** New tests pass; existing 9 tests still pass.

---

## Phase 2: API Convergence (breaking change with type alias)

### Task 3: New unified `check()` signature

**Files**:
- MODIFY: `crates/synthia-guardian/src/loop_detector.rs` (full rewrite of `LoopDetectorSet`)
- MODIFY: `crates/synthia-guardian/src/lib.rs` (re-exports)

**Steps**:

- [ ] **Step 3.1:** Rewrite `LoopDetectorSet` to include 5 detectors:
  ```rust
  pub struct LoopDetectorSet {
      doom_loop: DoomLoopDetector,
      generic_repeat: GenericRepeatDetector,
      ping_pong: PingPongDetector,
      poll_no_progress: PollNoProgressDetector,
      global_circuit: GlobalCircuitDetector,
  }

  impl LoopDetectorSet {
      pub fn new() -> Self {
          Self {
              doom_loop: DoomLoopDetector::new(),
              generic_repeat: GenericRepeatDetector::new(),
              ping_pong: PingPongDetector::new(),
              poll_no_progress: PollNoProgressDetector::new(),
              global_circuit: GlobalCircuitDetector::new(),
          }
      }

      pub fn check(
          &mut self,
          tool_name: &str,
          args_json: &str,
          iteration: usize,
      ) -> (LoopStatus, Option<LoopAction>) {
          if self.doom_loop.check(tool_name, args_json) {
              return (LoopStatus::Detected, Some(LoopAction::RequirePermission));
          }
          let (tool_id, args_hash) = hash_tool_args(tool_name, args_json);
          match self.generic_repeat.check(tool_id, args_hash) {
              LoopStatus::Ok => {}
              LoopStatus::Warning => return (LoopStatus::Warning, Some(LoopAction::Warn)),
              LoopStatus::Detected => return (LoopStatus::Detected, Some(LoopAction::Block)),
          }
          if self.ping_pong.check(tool_name) == LoopStatus::Detected {
              return (LoopStatus::Detected, Some(LoopAction::Block));
          }
          if iteration >= 30 {
              return (LoopStatus::Detected, Some(LoopAction::HardBlock));
          }
          (LoopStatus::Ok, None)
      }

      pub fn check_poll_result(&mut self, result: &str) -> LoopStatus {
          self.poll_no_progress.check(result)
      }

      pub fn reset(&mut self) {
          self.doom_loop.reset();
          self.generic_repeat.reset();
          self.ping_pong.reset();
          self.poll_no_progress.reset();
          self.global_circuit = GlobalCircuitDetector::new();
      }
  }
  ```

- [ ] **Step 3.2:** Update all 9 existing tests in `loop_detector.rs` to use new signature:
  ```rust
  // Before: set.check_tool_call("tool_a", "same_args")
  // After:  set.check("tool_a", "same_args", 0).0
  ```

- [ ] **Step 3.3:** Add 5 new tests for DoomLoop integration and full 5-detector flow.

- [ ] **Step 3.4:** Add type alias for backward compat (1 release):
  ```rust
  #[deprecated(note = "Use `check` instead")]
  pub fn check_tool_call(&mut self, tool_name: &str, args: &str) -> LoopDetectionResult {
      let (status, _) = self.check(tool_name, args, 0);
      // convert LoopStatus to LoopDetectionResult for backward compat
      // ...
  }
  ```

- [ ] **Step 3.5:** Run `cargo test -p synthia-guardian`; verify 14+ tests pass (9 migrated + 5 new).

- [ ] **Step 3.6:** Commit: `feat(guardian): unified 5-detector LoopDetectorSet with hash-based API`

**Verify:** 14+ tests pass; API signature is `check(tool, args, iteration) -> (LoopStatus, Option<LoopAction>)`.

---

## Phase 3: Migration (delete orphan)

### Task 4: Update synthia-agent to use synthia_guardian::LoopDetectorSet

**Files**:
- MODIFY: `crates/synthia-agent/src/dependencies.rs:21`
- MODIFY: `crates/synthia-agent/src/stream_builder/builder.rs:29-33, 228, 431`
- DELETE: `crates/synthia-agent/src/stream_builder/loop_detection.rs`
- MODIFY: `crates/synthia-agent/src/stream_builder/mod.rs`

**Steps**:

- [ ] **Step 4.1:** Update `dependencies.rs`:
  ```rust
  // Before: use crate::stream_builder::loop_detection::LoopDetectorSet;
  // After:  use synthia_guardian::LoopDetectorSet;
  ```

- [ ] **Step 4.2:** Update `builder.rs` for new API + RequirePermission handling:
  ```rust
  // Before:
  // let status = loop_detectors.check(&tu.name, &input_json_str, ctx.iteration);
  // if status == LoopStatus::Detected { loop_detected = true; }

  // After:
  let (status, action) = loop_detectors.check(&tu.name, &input_json_str, ctx.iteration);
  match (status, action) {
      (LoopStatus::Detected, Some(LoopAction::RequirePermission)) => {
          // Trigger permission.ask
          let decision = permission.ask(PermissionRequest::DoomLoop {
              tool: tu.name.clone(),
              args: tu.input.clone(),
          }).await?;
          if !decision.is_allow() {
              continue; // user denied
          }
      }
      (LoopStatus::Detected, _) => { loop_detected = true; }
      (LoopStatus::Warning, Some(LoopAction::Warn)) => {
          tracing::warn!(tool = %tu.name, "Loop warning");
      }
      (LoopStatus::Ok, _) => {}
  }
  ```

- [ ] **Step 4.3:** Delete `crates/synthia-agent/src/stream_builder/loop_detection.rs`.

- [ ] **Step 4.4:** Update `crates/synthia-agent/src/stream_builder/mod.rs`: remove `pub mod loop_detection;` line.

- [ ] **Step 4.5:** Run `cargo build -p synthia-agent`; verify no compile errors.

- [ ] **Step 4.6:** Run `cargo test -p synthia-agent`; verify all 476+ tests pass.

- [ ] **Step 4.7:** Commit: `refactor(agent): use synthia_guardian::LoopDetectorSet, delete local impl`

**Verify:** `grep -r "stream_builder::loop_detection" crates/` returns 0; `cargo test --workspace` all green.

---

### Task 5: E2E test for doom loop early-exit

**Files**:
- MODIFY: `crates/synthia-e2e/src/scenarios/loop_detection.rs`

**Steps**:

- [ ] **Step 5.1:** Add new e2e test:
  ```rust
  #[tokio::test]
  async fn test_doom_loop_triggers_permission_ask() {
      // Construct MockProvider that emits 3 identical read_file tool calls
      // Construct agent with mock Permission that records ask() invocations
      // Run agent loop
      // Assert: permission.ask was called with DoomLoop category
      // Assert: 3rd tool call was NOT executed
  }
  ```

- [ ] **Step 5.2:** Run `cargo test -p synthia-e2e --test loop_detection`; verify the new test passes.

- [ ] **Step 5.3:** Verify existing 5 e2e tests still pass (no regression).

- [ ] **Step 5.4:** Commit: `test(e2e): doom loop triggers permission.ask early-exit`

**Verify:** 6 e2e tests pass (5 existing + 1 new).

---

## Final Verification

### Task 6: Workspace-wide check

- [ ] **Step 6.1:** `cargo build --workspace --all-targets --all-features` → no errors.
- [ ] **Step 6.2:** `cargo test --workspace` → all 1183+ tests pass (1161 existing + 22 new).
- [ ] **Step 6.3:** `cargo clippy --all-targets --all-features --tests --all` → no new warnings.
- [ ] **Step 6.4:** `cargo +nightly fmt --all` → formatted.
- [ ] **Step 6.5:** `grep -r "stream_builder::loop_detection" crates/` → 0 results.
- [ ] **Step 6.6:** `grep -r "agent::loop_detector" crates/` → 0 results.
- [ ] **Step 6.7:** Add CHANGELOG entry: "LoopDetectorSet unified: doom loop now triggers permission.ask early-exit; all loop detection consolidated in synthia-guardian".
- [ ] **Step 6.8:** Final commit: `refactor(guardian): complete LoopDetectorSet unification + doom loop early-exit`

**Verify:** All checks pass; `git log` shows clean linear history with 6 commits.

---

## Risk Mitigation

| Risk | Mitigation | Rollback |
|------|-----------|----------|
| R1: DoomLoop behavior change | CHANGELOG entry + e2e test | Revert to `LoopAction::Block` fallback |
| R2: Breaking API change | Type alias for 1 release | Restore `check_tool_call` signature |
| R3: Agent test regression | 476+ tests must pass before delete | Keep `loop_detection.rs` as shim |
| R4: Trait abstraction reversal | `LoopAction` is enum, not trait | N/A (no regression) |

## References

- [design.md](file:///home/crochee/workspace/synthia/openspec/changes/loop-detector-unification/design.md) — design rationale
- [proposal.md](file:///home/crochee/workspace/synthia/openspec/changes/loop-detector-unification/proposal.md) — change summary
- [tasks.md](file:///home/crochee/workspace/synthia/openspec/changes/loop-detector-unification/tasks.md) — task list
- [opencode processor.ts:24, 296-331](file:///home/crochee/workspace/opencode/packages/opencode/src/session/processor.ts#L24) — doom loop inspiration
