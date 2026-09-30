# Retrospective: tool-output-offloading

> Written: 2026-06-23 (after verify passed)
> Commit range: N/A (changes remain uncommitted in worktree per project policy)
> Worktree: `.worktrees/tool-output-offloading/`

> **Update 2026-06-23**: Changes were committed after archive. §0 Evidence ("Commit range: N/A") and any claims relying on "uncommitted" state are superseded by worktree commit `cbe4cba` on branch `tool-output-offloading`.

---

## 0. Evidence

- **Commit range**: N/A — implementation kept in worktree; no commits created (project policy: commit only after explicit user instruction).
- **Diff size**: +366 / -24 lines across 14 files in worktree (`git diff --stat`)
- **Files touched**: `Cargo.lock`, `crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs`, `crates/synthia-agent/src/stream_builder/builder/tool_execution/execute.rs`, `crates/synthia-agent/src/stream_builder/steps/sample/tests.rs`, `crates/synthia-agent/tests/explicit_recovery_paths_test.rs`, `crates/synthia-agent/tests/step_sample_e2e_test.rs`, `crates/synthia-context/Cargo.toml`, `crates/synthia-context/src/truncate/mod.rs`, `crates/synthia-context/src/truncate/spill.rs`, `crates/synthia-context/src/truncate/tests.rs`, `crates/synthia-context/src/truncate/truncate_output.rs`, `crates/synthia-context/src/truncate/types.rs`, `crates/synthia-context/tests/compact_truncate_pipeline.rs`, `crates/synthia-context/tests/truncate_test.rs`; new file `crates/synthia-context/src/truncate/cleanup.rs`.
- **Tasks done**: 49/49 (all checkboxes in `tasks.md`)
- **Active hours**: ~1 session (apply phase completed within single agent run)
- **Subagent dispatches**: 0 explicit Task-tool subagents; implementation done inline by `openspec-apply-change` skill.
- **New external dependencies**: none at runtime; `tokio` features expanded to `["fs", "rt"]` (workspace), dev-dependency `filetime = "0.2"` added for cleanup tests.
- **Bugs encountered post-merge**: none (not merged).
- **OpenSpec validate state at archive**: PASS WITH WARNINGS — `tool-output-offloading` change valid; 4 pre-existing unrelated spec failures.
- **Test coverage signal**: `cargo +nightly fmt --all`, `cargo clippy --all-targets --all-features --tests --all`, and affected crate tests passed during verify.

Commit chain (時序): N/A — uncommitted worktree changes.

---

## 1. Wins

- [evidence: `crates/synthia-context/src/truncate/truncate_output.rs`, `types.rs`, `spill.rs`] Reused and extended the existing `truncate_output` unified entry point instead of creating a new `ToolOutputStore` crate, keeping the change small and aligned with project memory's "Truncate operations must be unified" rule.
- [evidence: `tasks.md` 49/49 complete] All planned tasks were completed in one apply pass without scope creep.
- [evidence: `spill.rs` `#[cfg(unix)]` block] Cross-platform file-permission issue was caught and fixed cleanly with conditional compilation.
- [evidence: `cleanup.rs` `spawn_blocking`] Cleanup is fire-and-forget async and does not block the ReAct loop.
- [evidence: tests in `crates/synthia-context/src/truncate/tests.rs`] Good unit-test coverage for thresholds, deterministic paths, permissions, and stale-file cleanup.

## 2. Misses

- 🟡 [painful | evidence: summary of errors in conversation] Several compile/test failures were fixed reactively instead of preventively: missing `Duration` import, `PermissionsExt` cross-platform use, unused `spill_path`/`sanitize_path_segment`, and two agent tests that used 50KB instead of 60KB data. These were all small but indicate the initial code passes were not fully clippy/test-clean.
- 🟡 [painful | evidence: `verify.md` §5] Implementation remains uncommitted, so there is no permanent commit history at archive time. This is by policy, but it complicates evidence-based retrospectives.
- 📌 [nit | evidence: `brainstorm.md` Open Questions] Two design questions (`truncate_messages` offload behavior, telemetry metrics) were deferred with open questions rather than resolved.

## 3. Plan deviations

| Plan task | What changed | Why |
|-----------|--------------|-----|
| 5.2 (cleanup after each tool execution phase) | Cleanup is spawned at session startup and inside the tool-execution path; exact hook point adjusted to match `execute_and_emit` control flow. | The `run` startup hook was the cleanest place for session-level cleanup; per-write cleanup is triggered inline where offload occurs. |
| 6 / 7 test placement | Some tests added to existing `tests.rs` and `truncate_test.rs` rather than strictly separated by unit vs integration. | Kept tests close to existing truncate test suite for discoverability. |

## 4. Skill / workflow compliance

| Skill                                            | Used |
|--------------------------------------------------|------|
| superpowers:brainstorming                        | ✓    |
| superpowers:writing-plans                        | ✓    |
| superpowers:using-git-worktrees                  | ✓    |
| superpowers:subagent-driven-development          | ✗    |
| (transitive) superpowers:test-driven-development | ✓    |
| (transitive) superpowers:requesting-code-review  | ✗    |
| superpowers:finishing-a-development-branch       | —    |

> **Default expectation**: 全部 ✓。每個 skill 都是 schema 設計的一部分, 跳過屬於異常情境。任一項 ✗ 都必須在下方 `### Deliberately Skipped Skills` subsection 提出原因與預防方案。

### Deliberately Skipped Skills

- **`superpowers:subagent-driven-development`**
  - **What was skipped**: Delegating implementation tasks to separate Task-tool subagents.
  - **Why this cycle**: The change scope was contained to a single crate (`synthia-context`) plus two small agent wiring points. `openspec-apply-change` was used inline and completed all tasks in one pass; spawning subagents would have added coordination overhead without reducing context pressure.
  - **How to prevent recurrence**: `scope-judgment rule` — use subagents when a change touches ≥3 independent modules or requires parallel exploration; for single-module enhancements with clear task lists, inline apply is acceptable.

- **`superpowers:requesting-code-review`**
  - **What was skipped**: Formal code-review subagent invocation before archive.
  - **Why this cycle**: The change is uncommitted and isolated in a worktree. Project policy defers commit until explicit user instruction, and no PR exists to review. Verification (`cargo fmt`, `clippy`, `test`) was used as the quality gate instead.
  - **How to prevent recurrence**: `CLAUDE.md trigger` — before any commit/merge, invoke `requesting-code-review` or manual human review. Since archive here does not involve merge, this is a boundary case.

- **`superpowers:finishing-a-development-branch`**
  - **What was skipped**: The entire skill (merge/PR/cleanup decisions).
  - **Why this cycle**: Changes are uncommitted; the skill is designed for completed branches ready to merge. It is not applicable until the user explicitly requests commit/merge.
  - **How to prevent recurrence**: `one-off — schema boundary case, no prevention possible`. This skill naturally belongs to the post-archive/post-commit phase, not to an uncommitted archive.

## 5. Surprises

- The existing `truncate_output` already had head/tail summarization and ULID-based spill logic, so the change was more about configuration and path determinism than building new behavior from scratch.
- `PermissionsExt` is platform-specific; the first attempt to set `0o600` unconditionally broke cross-platform compilation.
- Two existing agent tests (`explicit_recovery_paths_test.rs`, `step_sample_e2e_test.rs`) had hard-coded 50KB/60KB thresholds that needed adjustment because the new default `max_bytes` is 50KB.

## 6. Promote candidates → long-term learning

- [ ] 🟡 **Run `cargo clippy --all-targets --all-features --tests --all` before declaring a task complete** → **Promote to project CLAUDE.md** (verification section)
  > **Why**: Several small warnings/imports were only caught at the end, creating a tail of reactive fixes.
  > **How to apply**: Add a checkpoint in the apply-phase verification: every code-change task must be followed by `cargo check -p <crate>` or `cargo clippy` before moving to the next task.

- [ ] 📌 **Keep `max_bytes` defaults aligned with existing test fixtures** → **One-off** (record only)
  > **Why**: Changing a default threshold broke tests that happened to use nearby values; this is project-specific coupling, not a general rule.
  > **How to apply**: When touching default numeric constants, grep test suites for the old/new values and update them in the same change.

- [ ] 🔴 **Uncommitted worktrees make retrospectives evidence-poor** → **Promote to memory** (type: workflow)
  > **Why**: Without commits, diff stats and `git log` are unavailable, weakening the evidence-first retrospective requirement.
  > **How to apply**: When applying changes in a worktree under a "no auto-commit" policy, at least create local `WIP` commits inside the worktree before archive to preserve history; the user can still decide whether to push/rewrite them.
