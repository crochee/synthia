# Agent Bug Fix & Deduplication Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development
> to implement this plan task-by-task.

**Goal:** Fix 5 critical bugs (C1-C4, C6) and remove 3 sets of duplicate code in Synthia's agent code, per the 6-expert adversarial review (2026-06-10). Defer all 4 trait abstractions (D1-D4) for ≥6 months.

**Architecture:** Bug fixes are local to specific files; deduplication removes redundant implementations and unifies on existing active types. No new trait abstractions are introduced. The 4 new types (`CacheControlMark`, `CacheTtl`, `CacheScope`, `CommandBlacklist`) are simple data structs, not traits.

**Tech Stack:** Rust 1.x, `std::sync::Mutex` (replacing `RwLock`), `ahash` (replacing `DefaultHasher`), serde, tracing.

---

## Task 1: P1.1 — `CacheControlMark` independent hash (C1)

**Files**:
- NEW: `crates/synthia-context/src/prompt/mark.rs`
- MODIFY: `crates/synthia-context/src/prompt/cache.rs:233-237`
- MODIFY: `crates/synthia-context/src/lib.rs` (re-export)

**Steps**:

- [ ] **Step 1.1:** Create `crates/synthia-context/src/prompt/mark.rs` with struct definitions:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
  pub enum CacheTtl { #[default] Ephemeral, Extended, Long }

  #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
  pub struct CacheScope(pub String);

  impl CacheScope {
      pub fn new(user_id: &str, session_id: &str) -> Self {
          Self(format!("u={user_id};s={session_id}"))
      }
  }

  #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
  pub struct CacheControlMark {
      pub ttl: CacheTtl,
      pub scope: CacheScope,
      pub pinned: bool,
  }
  ```

- [ ] **Step 1.2:** Add unit test in `mark.rs`:
  ```rust
  #[test]
  fn cache_scope_distinguishes_users() {
      let a = CacheScope::new("alice", "s1");
      let b = CacheScope::new("bob", "s1");
      assert_ne!(a, b);
      assert!(a.0.contains("u=alice"));
      assert!(a.0.contains("s=s1"));
  }
  ```

- [ ] **Step 1.3:** Modify `cache.rs:233-237` `create_prompt_snapshot` to accept `cache_mark: &CacheControlMark`:
  ```rust
  pub fn create_prompt_snapshot(
      system_content: &str,
      tools_content: &str,
      model: &str,
      fast_mode: bool,
      cache_mark: &CacheControlMark,
  ) -> PromptStateSnapshot {
      let system_hash = compute_hash(system_content);
      let tools_hash = compute_hash(tools_content);
      let mut h = ahash::AHasher::default();
      cache_mark.ttl.hash(&mut h);
      cache_mark.scope.0.hash(&mut h);
      cache_mark.pinned.hash(&mut h);
      let cache_control_hash = h.finish();
      let prefix_hash = compute_hash(system_content);
      // ... rest unchanged
  }
  ```

- [ ] **Step 1.4:** Add `ahash` dependency to `crates/synthia-context/Cargo.toml` (if not present).

- [ ] **Step 1.5:** Re-export `CacheControlMark` in `crates/synthia-context/src/lib.rs`:
  ```rust
  pub use prompt::mark::{CacheControlMark, CacheScope, CacheTtl};
  ```

- [ ] **Step 1.6:** Add unit test in `cache.rs`:
  ```rust
  #[test]
  fn cache_control_hash_independent_of_system() {
      let mark1 = CacheControlMark::default();
      let mark2 = CacheControlMark { ttl: CacheTtl::Long, ..Default::default() };
      let s1 = create_prompt_snapshot("sys", "tools", "m", false, &mark1);
      let s2 = create_prompt_snapshot("sys", "tools", "m", false, &mark2);
      assert_eq!(s1.system_hash, s2.system_hash);
      assert_ne!(s1.cache_control_hash, s2.cache_control_hash);
  }
  ```

- [ ] **Step 1.7:** Update all callers of `create_prompt_snapshot` to pass `&CacheControlMark::default()` (or appropriate mark). Search via `grep -r "create_prompt_snapshot" crates/`.

- [ ] **Step 1.8:** Run `cargo test -p synthia-context`. All tests pass.

- [ ] **Step 1.9:** Run `cargo clippy -p synthia-context --all-targets --all-features --tests -- -D warnings`. Clean.

- [ ] **Step 1.10:** Commit: `fix(context): cache_control_hash independent of system_content (C1)`

---

## Task 2: P1.2 — `MergedPolicy` fail-closed default (C2)

**Files**:
- MODIFY: `crates/synthia-permission/src/merged_policy.rs:53-64`

**Steps**:

- [ ] **Step 2.1:** Add failing test in `merged_policy.rs`:
  ```rust
  #[test]
  fn unknown_pattern_asks() {
      let policy = MergedPolicy::default();
      assert_eq!(policy.evaluate("nonexistent_tool"), PermissionAction::Ask);
  }
  ```
  Run → FAILS (currently returns `Allow`).

- [ ] **Step 2.2:** Modify `merged_policy.rs:64`: change `unwrap_or(PermissionAction::Allow)` to `unwrap_or(PermissionAction::Ask)`.

- [ ] **Step 2.3:** Run test 2.1 → PASSES.

- [ ] **Step 2.4:** Add `PermissionAction::Ask` to doc comment of `MergedPolicy::evaluate`:
  ```rust
  /// Returns `PermissionAction::Ask` for any pattern not in the rules.
  /// This is intentional fail-closed behavior: unknown tools require
  /// explicit user confirmation. See ADR-2026-06-10.
  ```

- [ ] **Step 2.5:** Add CHANGELOG entry under current unreleased version:
  ```markdown
  ### BREAKING CHANGES
  - `MergedPolicy::evaluate` now returns `Ask` (not `Allow`) for unknown patterns.
    Migration: explicitly add `Allow` rules for all tools that should be silently allowed.
  ```

- [ ] **Step 2.6:** Run `cargo test -p synthia-permission`. All tests pass.

- [ ] **Step 2.7:** Commit: `fix(permission): MergedPolicy fail-closed default (C2)`

---

## Task 3: P1.4 — Fix `synthia-tool::exec::permission` compile error (C4)

**Files**:
- MODIFY: `crates/synthia-tool/src/exec/permission.rs`

**Steps**:

- [ ] **Step 3.1:** Run `cargo check -p synthia-tool --all-features 2>&1 | head -30` to confirm the compile error.

- [ ] **Step 3.2:** Replace local `PermissionPolicy` struct with `synthia_permission::MergedPolicy`:
  ```rust
  // Before
  use crate::types::PermissionLevel;  // non-existent type!
  pub struct PermissionPolicy { /* 4 levels */ }

  // After
  use synthia_permission::MergedPolicy;
  pub type PermissionPolicy = MergedPolicy;  // temporary alias
  ```

- [ ] **Step 3.3:** Update all usages within the file to call `MergedPolicy` methods directly (not the alias).

- [ ] **Step 3.4:** Run `cargo check -p synthia-tool --all-features 2>&1`. Should succeed (no "PermissionLevel not found" error).

- [ ] **Step 3.5:** Run `cargo test -p synthia-tool --all-features`. All tests pass.

- [ ] **Step 3.6:** Commit: `fix(tool): synthia-tool::exec::permission use synthia_permission (C4)`

---

## Task 4: P2.2 — Delete `synthia-permission::policy::PermissionPolicy` + `RuleSet`

**Files**:
- DELETE/MODIFY: `crates/synthia-permission/src/policy.rs`
- MODIFY: All callers in `crates/synthia-permission/`
- MODIFY: `crates/synthia-permission/src/lib.rs`

**Steps**:

- [ ] **Step 4.1:** Inventory callers: `grep -rn "PermissionPolicy\|RuleSet" crates/synthia-permission/`. Document the list.

- [ ] **Step 4.2:** Migrate each caller from `PermissionPolicy` (old struct) to `MergedPolicy`. Test files too.

- [ ] **Step 4.3:** Delete `RuleSet` struct and its impls from `policy.rs`.

- [ ] **Step 4.4:** Delete old `PermissionPolicy` struct from `policy.rs`. Keep only the file name (or rename to `merged_policy_alias.rs`).

- [ ] **Step 4.5:** Update `lib.rs`:
  ```rust
  // Before
  pub use policy::{PermissionPolicy, Permission, RuleSet};
  // After
  pub use merged_policy::MergedPolicy;
  pub use merged_policy::Permission as PermissionAction;  // or rename
  ```

- [ ] **Step 4.6:** Run `cargo test -p synthia-permission`. All 18+ migrated tests pass.

- [ ] **Step 4.7:** Verify `grep -r "pub struct PermissionPolicy" crates/synthia-permission/` returns 0.

- [ ] **Step 4.8:** Verify `grep -r "RuleSet" crates/synthia-permission/` returns 0.

- [ ] **Step 4.9:** Commit: `refactor(permission): unify to MergedPolicy, remove old PermissionPolicy + RuleSet`

---

## Task 5: P1.5 — `GenericRepeatDetector` O(1) HashMap counters (C6)

**Files**:
- MODIFY: `crates/synthia-agent/src/stream_builder/loop_detection.rs`

**Steps**:

- [ ] **Step 5.1:** Add failing test for zero allocation:
  ```rust
  #[test]
  fn check_does_not_allocate_string() {
      let mut det = GenericRepeatDetector::new(20);
      // No &str input — only u64s
      let status = det.check(0xABCD, 0x1234);
      assert_eq!(status, LoopStatus::Ok);
  }
  ```

- [ ] **Step 5.2:** Rewrite `GenericRepeatDetector`:
  ```rust
  pub struct GenericRepeatDetector {
      counts: HashMap<(u64, u64), u32>,
      max_threshold: u32,
  }

  impl GenericRepeatDetector {
      pub fn new(max_threshold: u32) -> Self {
          Self { counts: HashMap::new(), max_threshold }
      }
      pub fn check(&mut self, tool_id: u64, args_hash: u64) -> LoopStatus {
          let entry = self.counts.entry((tool_id, args_hash)).or_insert(0);
          *entry += 1;
          if *entry >= self.max_threshold {
              LoopStatus::Detected
          } else {
              LoopStatus::Ok
          }
      }
      pub fn record_outcome(&mut self, tool_id: u64, args_hash: u64, success: bool) {
          if success {
              if let Some(c) = self.counts.get_mut(&(tool_id, args_hash)) {
                  *c = c.saturating_sub(1);
                  if *c == 0 { self.counts.remove(&(tool_id, args_hash)); }
              }
          }
      }
  }
  ```

- [ ] **Step 5.3:** Update all callers to pass `tool_id: u64, args_hash: u64`. Add helper:
  ```rust
  pub fn hash_tool_args(tool_name: &str, args_json: &str) -> (u64, u64) {
      use ahash::AHasher;
      use std::hash::{Hash, Hasher};
      let mut h1 = AHasher::default();
      tool_name.hash(&mut h1);
      let mut h2 = AHasher::default();
      args_json.hash(&mut h2);
      (h1.finish(), h2.finish())
  }
  ```

- [ ] **Step 5.4:** Run `cargo test -p synthia-agent` for `test_generic_repeat_*` tests. Fix any that assumed VecDeque semantics.

- [ ] **Step 5.5:** Add benchmark: `criterion` benchmark for `GenericRepeatDetector::check` < 100 ns/op.

- [ ] **Step 5.6:** Commit: `perf(agent): GenericRepeatDetector O(1) HashMap counters (C6)`

---

## Task 6: P2.1 — Delete `synthia-agent::agent::loop_detector::LoopDetector`

**Files**:
- DELETE: `crates/synthia-agent/src/agent/loop_detector.rs`
- MODIFY: `crates/synthia-agent/src/agent/core.rs:77`
- MODIFY: `crates/synthia-agent/src/agent/react.rs:557-706` (6 call sites)
- MODIFY: `crates/synthia-agent/src/agent/step.rs:489`
- MODIFY: `crates/synthia-agent/Cargo.toml` (if needed)
- MODIFY: `crates/synthia-guardian/src/loop_detector.rs` (make `pub`)

**Steps**:

- [ ] **Step 6.1:** Make `LoopDetectorSet` `pub` in `crates/synthia-guardian/src/loop_detector.rs`:
  ```rust
  pub struct LoopDetectorSet { /* ... */ }
  impl LoopDetectorSet {
      pub fn new(/* ... */) -> Self { /* ... */ }
      // make all methods pub
  }
  ```

- [ ] **Step 6.2:** Add `synthia-guardian` dependency to `crates/synthia-agent/Cargo.toml` if not present.

- [ ] **Step 6.3:** In `agent/core.rs:77`, change:
  ```rust
  // Before
  pub loop_detector: Arc<RwLock<LoopDetector>>,
  // After
  pub loop_detector: Arc<Mutex<LoopDetectorSet>>,
  ```

- [ ] **Step 6.4:** Update `Agent::new` to construct `LoopDetectorSet` (use `synthia_guardian::loop_detector::LoopDetectorSet::new(...)`).

- [ ] **Step 6.5:** Update 6 call sites in `react.rs:557-706`. Each call site changes method name (e.g., `record` → `record_tool_call` or similar).

- [ ] **Step 6.6:** Update `step.rs:489` to use `LoopDetectorSet` methods.

- [ ] **Step 6.7:** Run `cargo build -p synthia-agent`. Verify no compile errors.

- [ ] **Step 6.8:** Run `cargo test -p synthia-agent`. Verify all 30+ tests pass (most are in the soon-to-be-deleted file; run them in their current location first).

- [ ] **Step 6.9:** Delete `crates/synthia-agent/src/agent/loop_detector.rs`.

- [ ] **Step 6.10:** Move 30+ tests from deleted file to `crates/synthia-guardian/src/loop_detector.rs` (or new `crates/synthia-agent/tests/loop_detector_integration.rs`).

- [ ] **Step 6.11:** Run `cargo test -p synthia-agent` again. All tests pass.

- [ ] **Step 6.12:** Verify `grep -r "pub struct LoopDetector" crates/` returns exactly 1 result (`synthia-guardian::loop_detector::LoopDetectorSet`).

- [ ] **Step 6.13:** Commit: `refactor(agent): unify to LoopDetectorSet, delete agent::LoopDetector`

---

## Task 7: P1.3 — `try_write` → `Mutex` (C3)

**Files**:
- MODIFY: `crates/synthia-agent/src/agent/step.rs:489-491`
- (Already done in Task 6: `core.rs:77` switched to `Mutex`)

**Steps**:

- [ ] **Step 7.1:** Verify Task 6 changed `core.rs:77` to `Arc<Mutex<LoopDetectorSet>>`. If not, do it now.

- [ ] **Step 7.2:** In `step.rs:489`, replace:
  ```rust
  // Before
  if let Ok(mut guard) = agent.loop_detector.try_write() {
      guard.record(pattern);
  }
  // After
  let mut guard = agent.loop_detector.lock().expect("loop_detector mutex poisoned");
  guard.record(pattern);
  ```

- [ ] **Step 7.3:** Add loom test:
  ```rust
  #[cfg(loom)]
  #[test]
  fn record_under_concurrent_detect() {
      loom::model(|| {
          let det = Arc::new(Mutex::new(LoopDetectorSet::new()));
          let d1 = det.clone();
          let d2 = det.clone();
          let h1 = loom::thread::spawn(move || {
              d1.lock().unwrap().record_tool_call(/* pattern */);
          });
          let h2 = loom::thread::spawn(move || {
              let _g = d2.lock().unwrap();
              // detect_loop
          });
          h1.join().unwrap(); h2.join().unwrap();
      });
  }
  ```

- [ ] **Step 7.4:** Run `cargo test -p synthia-agent` (with `--features loom` if needed). All tests pass.

- [ ] **Step 7.5:** Verify `grep -r "try_write" crates/synthia-agent/` returns 0 for loop_detector.

- [ ] **Step 7.6:** Commit: `fix(agent): try_write → Mutex, no silent record drops (C3)`

---

## Task 8: P2.3 — Rename `synthia_exec::sandbox` → `command_blacklist`

**Files**:
- NEW: `crates/synthia-exec/src/command_blacklist.rs`
- DELETE: `crates/synthia-exec/src/sandbox.rs`
- MODIFY: `crates/synthia-exec/src/lib.rs`
- MODIFY: All callers (e.g., `crates/synthia-exec/src/bash_tool.rs`)

**Steps**:

- [ ] **Step 8.1:** Create `crates/synthia-exec/src/command_blacklist.rs`:
  ```rust
  //! String-match command blacklist.
  //!
  //! **NOT an OS-level sandbox.** This module does NOT prevent malicious
  //! commands that bypass pattern matching (e.g., unicode obfuscation,
  //! base64 encoding, `r""m"` syntax). For real sandboxing, see
  //! `synthia-sandbox-linux` (future).
  //!
  //! Use only as a defensive layer for obvious dangerous patterns.

  pub const BLACKLISTED_PATTERNS: &[&str] = &[ /* 25+ patterns */ ];

  pub struct CommandBlacklist { /* ... */ }

  impl CommandBlacklist {
      pub fn is_command_blacklisted(&self, command: &str) -> bool { /* ... */ }
  }

  #[deprecated(note = "Use CommandBlacklist instead")]
  pub type Sandbox = CommandBlacklist;
  ```

- [ ] **Step 8.2:** Update `lib.rs`:
  ```rust
  // Before
  pub mod sandbox;
  pub use sandbox::Sandbox;
  // After
  pub mod command_blacklist;
  pub use command_blacklist::{CommandBlacklist, BLACKLISTED_PATTERNS};
  ```

- [ ] **Step 8.3:** Update `bash_tool.rs`:
  ```rust
  // Before
  use crate::sandbox::Sandbox;
  pub struct BashTool { sandbox: Sandbox }
  // After
  use crate::command_blacklist::CommandBlacklist;
  pub struct BashTool { sandbox: CommandBlacklist }
  ```

- [ ] **Step 8.4:** Delete `crates/synthia-exec/src/sandbox.rs`.

- [ ] **Step 8.5:** Run `cargo build -p synthia-exec --all-targets --all-features`. Verify build succeeds.

- [ ] **Step 8.6:** Run `cargo test -p synthia-exec`. All tests pass.

- [ ] **Step 8.7:** Verify `grep -r "pub mod sandbox" crates/synthia-exec/` returns 0.

- [ ] **Step 8.8:** Commit: `refactor(exec): rename sandbox to command_blacklist, honest naming`

---

## Task 9: Final verification and ADR

**Steps**:

- [ ] **Step 9.1:** Run full workspace build: `cargo build --all-targets --all-features`.

- [ ] **Step 9.2:** Run full test suite: `cargo test --all`.

- [ ] **Step 9.3:** Run clippy: `cargo clippy --all-targets --all-features --tests --all -- -D warnings`. Fix any warnings.

- [ ] **Step 9.4:** Run formatter: `cargo +nightly fmt --all`.

- [ ] **Step 9.5:** Verify `grep -r "try_write" crates/synthia-agent/` returns 0.

- [ ] **Step 9.6:** Verify `grep -r "pub struct LoopDetector" crates/` returns 1 (only `LoopDetectorSet`).

- [ ] **Step 9.7:** Verify `grep -r "pub struct PermissionPolicy" crates/synthia-permission/` returns 0.

- [ ] **Step 9.8:** Verify `grep -r "pub mod sandbox" crates/synthia-exec/` returns 0.

- [ ] **Step 9.9:** Add ADR comment to `MergedPolicy` doc:
  ```rust
  //! # ADR-2026-06-10
  //!
  //! This is the unified permission policy after 6-expert adversarial review
  //! (R1 Architect, R2 Security, R3 Performance, R4 Rust, R5 Concurrency, R6 Devil's Advocate).
  //!
  //! Trait abstraction (D1-D4) was rejected as over-engineered.
  //! See `docs/superpowers/specs/2026-06-10-agent-bug-fix-and-dedup-design.md` for details.
  //!
  //! Re-evaluation of trait abstraction is scheduled for 6 months from this date.
  ```

- [ ] **Step 9.10:** Commit: `chore: Phase 1+2 complete, Phase 3 deferred 6 months`

---

## Verification Checklist

After all tasks complete:

- [ ] All 5 critical bugs (C1-C4, C6) are fixed
- [ ] 3 sets of duplicate code are removed (`LoopDetector` → 1, `PermissionPolicy` → 1, `Sandbox` → `CommandBlacklist` rename)
- [ ] No silent record drops (`try_write` → `Mutex`)
- [ ] No fail-open defaults (`MergedPolicy::evaluate(unknown)` → `Ask`)
- [ ] No O(N) hot path algorithms in `GenericRepeatDetector` (now O(1))
- [ ] `CacheControlMark` carries `user_id` namespace (cross-session leak prevented)
- [ ] All 4 trait abstractions (D1-D4) are NOT implemented
- [ ] Re-evaluation criteria documented in code comments
- [ ] CHANGELOG entry for `MergedPolicy` breaking change
