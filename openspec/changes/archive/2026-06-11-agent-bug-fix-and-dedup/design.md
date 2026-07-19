## Context

### Background

Synthia is a Rust AI Agent. Comparison with production-grade agents (opencode, codex) revealed apparent duplication: 3× LoopDetector, 3× Permission, 2× Sandbox, 2× ReAct loop, 2× Circuit Breaker. An initial design proposed 4 trait abstractions (D1-D4) to unify them.

### 6-Expert Adversarial Review

Before implementing, 6 domain experts (R1 Architect, R2 Security, R3 Performance, R4 Rust, R5 Concurrency, R6 Devil's Advocate) were dispatched in parallel to adversarially review D1-D4. **The "consensus" was fragile**: 4/6 reject D3, 3/6 reject D2 and D4, all 6 reject the framing.

All 6 experts converged on a meta-conclusion: **the problem statements are real, but the trait abstraction solutions are over-engineered**. There are 5 critical bugs and 3 sets of duplicate code that should be fixed first; trait abstraction should be deferred ≥6 months.

### Current State

- `synthia-context/src/prompt/cache.rs:235` — `cache_control_hash` reuses `system_content` (bug C1)
- `synthia-permission/src/merged_policy.rs:53-64` — `evaluate(unknown) = Allow` fail-open (bug C2)
- `synthia-agent/src/agent/step.rs:489` — `try_write` silently drops records (bug C3)
- `synthia-tool/src/exec/permission.rs` — references non-existent `PermissionLevel` type (bug C4)
- `synthia-agent/src/stream_builder/loop_detection.rs:53-57,215` — O(N) filter + JSON clone (perf C6)
- 3 implementations of `LoopDetector` (guardian, agent, stream_builder)
- 4 implementations of `PermissionPolicy` (permission/policy, permission/merged, tool/exec/permission, agent/fork_policy)
- 2 sandbox implementations with mismatched semantics (string blacklist vs policy executor)
- `OsSandbox` trait does not exist; `wrap_command` API is from-scratch

### Stakeholders

- `synthia-permission` users (most affected by permission refactor)
- macOS developers (need protection from Linux-centric design)
- OpenAI-only users (need protection from Anthropic-centric cache design)
- Cache hit-rate operators (need to validate whether LongTtl is worth doing)

## Goals / Non-Goals

### Goals

1. Fix 5 critical bugs (C1-C4, C6) that affect security, correctness, and performance
2. Reduce duplicate code from 3 sets of `LoopDetector` to 1, 4 sets of `PermissionPolicy` to 1
3. Rename misleading "sandbox" to honest "command_blacklist"
4. Establish re-evaluation criteria for future trait abstraction
5. Improve P6 (Distrust by Default) compliance by removing false abstraction promises

### Non-Goals

1. **Trait abstractions (D1-D4) are explicitly NOT in this change.** Defer to separate changes after re-evaluation criteria are met.
2. **No new OS-level sandbox (Landlock/seccomp/JobObject).** The "sandbox" code remains a string-match blacklist, just honestly named.
3. **No Provider-specific cache control API changes.** The `cache_control_hash` bug is fixed; whether to add explicit `CacheBreakpoint` markers is a separate decision gated on hit-rate telemetry.
4. **No breaking change to public API beyond `MergedPolicy` default (`Allow` → `Ask`).** All other refactors are internal or additive.

## Decisions

### D1: Phase 1 Bug Fixes (5 items)

#### D1.1 — Fix `cache_control_hash` independent hash
- **Choice**: Introduce `CacheControlMark` struct; hash it independently of `system_content`. Enforce `user_id` in `CacheScope` via Debug/assert.
- **Rationale**: C1 is a real bug — `cache_control_hash = system_hash` means `CacheBreakDetector` cannot detect cache_control changes. Fixing it without the struct leaks: any future cache_control field will regress.
- **Alternative considered**: Just rename `cache_control_hash` to `system_hash_2` (skip-the-struct). Rejected: the field needs to carry actual cache_control info, not be a duplicate of system_hash.

#### D1.2 — `MergedPolicy::evaluate` fail-closed default
- **Choice**: Default unknown pattern → `Ask` (require confirmation), not `Allow`.
- **Rationale**: C2 is a CVE-level fail-open. Aligns `MergedPolicy` with `PermissionPolicy::default()` which already uses `RequireConfirm`.
- **Alternative considered**: Keep `Allow` default and add explicit `force_deny` flag. Rejected: makes "unknown tool" semantics ambiguous; fail-closed is a single, simple rule.

#### D1.3 — `try_write` → `Mutex<LoopDetector>`
- **Choice**: Replace `Arc<RwLock<LoopDetector>>` with `Arc<Mutex<LoopDetector>>` in `agent/core.rs:77`. Replace `try_write` with `lock().expect()` in `step.rs:489`.
- **Rationale**: All `LoopDetector` operations require write access; `RwLock` write starvation risk; `Mutex` has 1 atomic op fast path.
- **Alternative considered**: Keep `RwLock`, change `try_write` to `blocking_write`. Rejected: still has 2 atomic ops in fast path, still has write starvation risk under concurrent reads.

#### D1.4 — Fix `synthia-tool::exec::permission` compile error
- **Choice**: Migrate `synthia-tool/src/exec/permission.rs` to use `synthia_permission::Permission`. Delete local `PermissionPolicy` struct.
- **Rationale**: Bug C4 is a real compile error hidden behind feature flags. The local `PermissionPolicy` is the 4th redundant implementation.
- **Alternative considered**: Add `PermissionLevel` to `synthia-tool::types`. Rejected: perpetuates the 4-impl mess.

#### D1.5 — O(1) `GenericRepeatDetector` with HashMap counters
- **Choice**: Replace `VecDeque<(String, u64)>` with `HashMap<(u64, u64), u32>`; drop `input_json.to_string()` clone.
- **Rationale**: C6 is 3-5 ms/task wasted on O(N) filter + N² HashMap rebuild + full JSON clone. HashMap is O(1) per check, O(1) per outcome, zero String allocation.
- **Alternative considered**: Keep VecDeque, just drop the clone. Rejected: still O(N) per check; doesn't fix the main perf issue.

### D2: Phase 2 Deduplication (3 items)

#### D2.1 — Delete `synthia-agent::agent::loop_detector::LoopDetector`
- **Choice**: Delete the file. Make `LoopDetectorSet` `pub` from `synthia-guardian`. Re-export in `synthia-agent`. Update 6 call sites in `react.rs`.
- **Rationale**: `agent::LoopDetector` is a frozen snapshot with 3 detectors; `LoopDetectorSet` has 4 detectors and is actively maintained. Both implement the same algorithm.
- **Alternative considered**: Keep both, add a `LoopDetector` trait to unify. Rejected: R1 argued `LoopDetectorSet` is structural (单态化); trait adds vtable cost. R6 agreed.

#### D2.2 — Delete `synthia-permission::policy::PermissionPolicy` + `RuleSet`
- **Choice**: Delete the old `PermissionPolicy` struct in [policy.rs:1-157](file:///home/crochee/workspace/synthia/crates/synthia-permission/src/policy.rs#L1-L157). Keep `MergedPolicy` as the only public type. Delete `RuleSet` compat adapter.
- **Rationale**: `MergedPolicy` is the active model; `RuleSet` exists only as a compat shim for the old struct (R6: "Chesterton's Fence in reverse"). 18+ tests in `permission/` use the old struct and need migration.
- **Alternative considered**: Add a sub-trait (D2 original) to keep both. Rejected: R2/R6 strongly argued this is debt on debt; the right answer is to delete.

#### D2.3 — Rename `synthia_exec::sandbox` → `command_blacklist`
- **Choice**: Rename module `sandbox` → `command_blacklist`, struct `Sandbox` → `CommandBlacklist`, method `is_command_allowed` → `is_command_blacklisted`.
- **Rationale**: The current code is a 25-pattern string-match blacklist, not an OS sandbox. R2/R5 both flagged this as a security anti-pattern. Renaming clarifies the security level.
- **Alternative considered**: Keep name, add `# Security` doc warning. Rejected: type names matter; downstream code uses `use synthia_exec::sandbox::Sandbox` and grep-ability matters.

### D3: Phase 3 Trait Abstraction Deferral

#### D3.1 — D1 `LoopDetector` trait: DEFERRED
- **Choice**: Do not create a trait. Keep `LoopDetectorSet` as-is.
- **Re-evaluation criteria** (all of these must be true):
  - `DoomLoopDetector` needs cross-process state (currently process-local)
  - Plugin authors request pluggable detectors
  - ≥3 distinct loop detection strategies need to coexist

#### D3.2 — D2 `PermissionPolicy` sub-traits: DEFERRED
- **Choice**: Do not create sub-traits. `MergedPolicy` is the only implementation.
- **Re-evaluation criteria**:
  - Mutable path becomes a hot path (currently cold)
  - Lock contention in `Mutex<Policy>` measured >1% CPU
  - Plugin authors need to inject custom policies

#### D3.3 — D3 `OsSandbox` trait: DEFERRED
- **Choice**: Do not create a trait. Future Linux Landlock implementation is a single concrete struct.
- **Re-evaluation criteria** (strongest gate):
  - At least 1 platform has a real implementation
  - Second platform reaches prototype
  - Production telemetry shows OS-level sandbox blocks a real attack

#### D3.4 — D4 `Message::cache_control`: DEFERRED
- **Choice**: Do not add `cache_control` field to `Message`. `CacheBreakDetector` tracks hit rate; if <70%, reconsider.
- **Re-evaluation criteria**:
  - Cache hit rate <70% with `TwoPartPrompt` only
  - Provider API stabilizes (no breaking changes in 3 months)
  - 2+ providers in active use require distinct cache control

## Risks / Trade-offs

### Risks

- [Risk] **P1.2 breaking change** — Changing `MergedPolicy::evaluate` default from `Allow` to `Ask` may disrupt user workflows that relied on silent allow.
  → Mitigation: Document in CHANGELOG; provide migration: explicitly add `Allow` rules for all previously-unknown tools. Telemetry: count `Ask` decisions to measure impact.

- [Risk] **P1.5 algorithm change** — HashMap-based `GenericRepeatDetector` is semantically different (decay model vs. window-based). Existing tests may assume window semantics.
  → Mitigation: Run full test suite; explicitly test decay behavior. Add regression tests for: "occasional success breaks the loop" + "30-step max loop still triggers".

- [Risk] **P2.1 deletion breaks external callers** — `synthia-agent::agent::loop_detector::LoopDetector` may be referenced in 3rd-party plugins.
  → Mitigation: Grep workspace + git history for all references; provide deprecation warning for 1 release cycle.

- [Risk] **P2.2 deletion breaks 18+ tests** — Tests in `permission/` use old `PermissionPolicy` struct.
  → Mitigation: Migrate tests in same PR; provide `Migration Cookbook` with diff examples.

- [Risk] **P2.3 rename breaks downstream imports** — `use synthia_exec::sandbox::Sandbox` will fail.
  → Mitigation: Provide type alias `pub type Sandbox = CommandBlacklist;` for 1 release; update docs.

- [Risk] **Deferral never re-evaluated** — Trait abstraction stays on the shelf indefinitely.
  → Mitigation: Set explicit calendar reminder; document re-evaluation criteria in code comments; add "Phase 3" section to README.

### Trade-offs

- [Trade-off] **No trait abstraction in this change** → 6-month delay for any clear need.
  → Accepted: R1/R2/R5/R6 all agreed that premature abstraction is worse than delayed abstraction. Re-evaluation criteria provide escape valve.

- [Trade-off] **3-4 weeks of refactoring work for bug fixes** — Phase 2 deletions take longer than just fixing bugs.
  → Accepted: Reduces long-term maintenance cost; aligns with P10 (File System as Memory — fewer duplicates = simpler mental model).

- [Trade-off] **Fail-closed default may add user friction** — Every unknown tool now asks for confirmation.
  → Accepted: Security > convenience. Users can pre-register Allow rules for known tools.

## Migration Plan

### Pre-conditions

- All 5 bugs in `synthia-agent` and `synthia-permission` reproduced as failing tests
- Test suite passes on main branch baseline

### Phase 1: Bug Fixes (1-2 days)

1. **P1.1** — Add `CacheControlMark` struct in `synthia-context/src/prompt/mark.rs` (new file). Update `cache.rs:233-237` to use independent hash. Add unit tests.
2. **P1.2** — Change `merged_policy.rs:64` from `Allow` to `Ask`. Update existing tests. Add CHANGELOG entry.
3. **P1.3** — Change `core.rs:77` to `Arc<Mutex<LoopDetector>>`. Update `step.rs:489` to `lock().expect()`. Verify loom test.
4. **P1.4** — Migrate `synthia-tool/src/exec/permission.rs` to `synthia_permission::Permission`. Delete local struct.
5. **P1.5** — Rewrite `GenericRepeatDetector` with `HashMap` counters. Update existing tests.

**Verification**: `cargo build --all-targets --all-features && cargo test --all && cargo clippy --all-targets --all-features --tests --all`

### Phase 2: Deduplication (2-4 weeks)

1. **P2.1** — Make `LoopDetectorSet` `pub` in `synthia-guardian`. Update `synthia-agent/Cargo.toml`. Update `core.rs`, `react.rs`, `step.rs`. Delete `agent/loop_detector.rs`. Move 30+ tests to `synthia-guardian`.
2. **P2.2** — Inventory `PermissionPolicy` callers via grep. Migrate each to `MergedPolicy`. Delete `policy.rs:1-157`. Update 18+ tests.
3. **P2.3** — Rename module/struct/method in `synthia-exec/src/sandbox.rs`. Update all callers. Add type alias for 1 release.

**Verification**: `cargo build` green; `cargo test` green; no duplicate code (grep returns 0).

### Phase 3: Trait Abstraction Deferral (Calendar trigger 6 months out)

1. Add "Phase 3 re-evaluation" task to backlog.
2. Document re-evaluation criteria in code comments.
3. Wait for calendar trigger; do not implement unless criteria met.

### Rollback

- **P1.2 rollback**: Revert `merged_policy.rs:64` to `Allow`. Tests should still pass.
- **P1.5 rollback**: Revert `GenericRepeatDetector` to `VecDeque` impl. Performance regression acceptable for 1 release.
- **P2.1/P2.2/P2.3 rollback**: Git revert. Destructive to other refactors; coordinate with team.

## Open Questions

1. **P1.2 user migration** — Do we need a transition period where `MergedPolicy::evaluate` returns `Ask` but emits a `deprecation_warning` event? (Suggested: yes, for 1 release.)
2. **P1.5 algorithm semantics** — Should the decay model match the old window semantics exactly, or is this a "bug fix" that intentionally changes behavior? (Need product owner sign-off.)
3. **P2.1 external callers** — How do we detect 3rd-party callers of `synthia-agent::agent::loop_detector::LoopDetector`? (Grep crates.io? Out of scope for this change?)
4. **P2.2 test migration order** — Migrate 18+ tests in one PR or split into multiple? (Lean: one PR for atomic review.)
5. **Phase 3 calendar trigger** — 6 months from now, or align with next major release? (Lean: 6 months from now, regardless of release.)
