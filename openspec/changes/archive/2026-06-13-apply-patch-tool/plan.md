# Apply Patch Tool Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development
> to implement this plan task-by-task.

**Goal:** Implement Anthropic V4A `apply_patch` tool in synthia-tool as an atomic multi-file mutation primitive, exposed alongside the existing `multi_edit` tool.

**Architecture:** New builtin tool `ApplyPatchTool` in `crates/synthia-tool/src/builtin/apply_patch.rs`, with a dedicated V4A parser module at `crates/synthia-tool/src/builtin/v4a.rs`. The parser returns `Vec<PatchOp>`; the tool applies them via a snapshot-then-dry-run-then-commit pipeline to guarantee atomicity. Registration is one-line in `register_defaults()`. Path safety and concurrency reuse existing primitives.

**Tech Stack:** Rust, async-trait, serde, standard library only (no new dependencies).

---

## Task 1: V4A Parser — data structures

**Files:** `crates/synthia-tool/src/builtin/v4a.rs` (new)

- [ ] **Step 1.1:** Create `v4a.rs` with module-level docs explaining the V4A spec
- [ ] **Step 1.2:** Define `PatchOp` enum with `Update { path, hunks }`, `Add { path, content }`, `Delete { path }`, `Move { from, to }` variants. Derive `Debug, Clone, PartialEq`
- [ ] **Step 1.3:** Define `Hunk` struct with `context_before: Vec<String>`, `deletions: Vec<String>`, `insertions: Vec<String>`, `context_after: Vec<String>`, `ends_at_eof: bool`. Derive `Debug, Clone, PartialEq`
- [ ] **Step 1.4:** Define `ParseError` enum with `MissingBeginMarker`, `MissingEndMarker`, `UnknownOpHeader(String)`, `MoveOutsideUpdate`, `HunkOutOfOrder`, `EmptyHunk`, `IoError(String)`. Derive `Debug, Clone, PartialEq`
- [ ] **Step 1.5:** Run `cargo check -p synthia-tool` to confirm the module compiles

**Commit:** `feat(tool): scaffold V4A parser data structures`

---

## Task 2: V4A Parser — `parse_v4a` implementation (TDD)

**Files:** `crates/synthia-tool/src/builtin/v4a.rs`

- [ ] **Step 2.1:** Add `mod tests` submodule with `#[test] fn test_parse_v4a_valid_update()` — input: a 5-line V4A Update with one hunk. Expected: 1 `PatchOp::Update` with 1 hunk
- [ ] **Step 2.2:** Run test — it should fail to compile because `parse_v4a` doesn't exist
- [ ] **Step 2.3:** Add stub `pub fn parse_v4a(_input: &str) -> Result<Vec<PatchOp>, ParseError> { unimplemented!() }`
- [ ] **Step 2.4:** Run test — it should panic (not yet implemented)
- [ ] **Step 2.5:** Implement the state machine: scan lines, accumulate state (`ExpectBeginMarker` / `InOp` / `ExpectEndMarker`), match on line prefixes
- [ ] **Step 2.6:** Run test — it should pass
- [ ] **Step 2.7:** Add 5 more tests: `test_parse_v4a_valid_add`, `test_parse_v4a_valid_delete`, `test_parse_v4a_valid_move`, `test_parse_v4a_missing_begin_marker`, `test_parse_v4a_move_outside_update`. Each follows write-fail-implement-pass cycle
- [ ] **Step 2.8:** Run `cargo test -p synthia-tool v4a::` — all 6 tests pass

**Commit:** `feat(tool): implement V4A parser with state machine`

---

## Task 3: ApplyPatchTool scaffold + dry-run logic (TDD)

**Files:** `crates/synthia-tool/src/builtin/apply_patch.rs` (new)

- [ ] **Step 3.1:** Create `apply_patch.rs` with `use` statements for `crate::traits::Tool`, `crate::types::{ToolInput, ToolOutput}`, `crate::builtin::path::{check_path_safety, resolve_path}`, `crate::builtin::v4a::{parse_v4a, PatchOp}`, and `async_trait`
- [ ] **Step 3.2:** Define `pub struct ApplyPatchTool;` with `impl Default for ApplyPatchTool`
- [ ] **Step 3.3:** Implement `Tool` trait stubs: `name = "apply_patch"`, `description = "Apply an Anthropic V4A patch to multiple files atomically..."`, parameters schema `{ "patch": { "type": "string" } }`, `requires_permission = true`, `is_concurrency_safe = false`
- [ ] **Step 3.4:** Stub `async fn call` to return `ToolOutput::error("not yet implemented")`
- [ ] **Step 3.5:** Run `cargo check -p synthia-tool` — compiles
- [ ] **Step 3.6:** Add `mod tests` with `#[tokio::test] fn test_apply_patch_update_single_hunk()` — write a V4A Update for a temp file, call `tool.call()`, assert success and the file content matches expected
- [ ] **Step 3.7:** Run test — fails (not implemented)
- [ ] **Step 3.8:** Implement snapshot logic: `HashMap<PathBuf, Option<String>>` storing pre-patch state (None = new file)
- [ ] **Step 3.9:** Implement dry-run logic: walk `Vec<PatchOp>`, for each Update validate hunk matches against snapshot, for Add validate target doesn't exist, for Delete validate source exists, for Move validate source exists + target doesn't
- [ ] **Step 3.10:** Run test — passes
- [ ] **Step 3.11:** Implement commit logic: iterate ops in source order, for each apply filesystem write; on any commit failure restore all prior files from snapshot
- [ ] **Step 3.12:** Run test — still passes

**Commit:** `feat(tool): implement ApplyPatchTool with snapshot/dry-run/commit pipeline`

---

## Task 4: ApplyPatchTool — additional test cases (TDD)

**Files:** `crates/synthia-tool/src/builtin/apply_patch.rs` mod tests

For each test below: write test → run (fails) → implement missing logic if needed → run (passes):

- [ ] **Step 4.1:** `test_apply_patch_add_and_delete` — V4A with `*** Add File: new.txt` (full content) and `*** Delete File: old.txt`. Assert both ops committed
- [ ] **Step 4.2:** `test_apply_patch_move_across_dirs` — V4A with Update + `*** Move to: subdir/moved.txt`. Assert file at new path, original path absent
- [ ] **Step 4.3:** `test_apply_patch_rollback_on_hunk_mismatch` — V4A with 2 ops: first Update matches, second Update's hunk doesn't match. Assert first op's file unchanged (rolled back)
- [ ] **Step 4.4:** `test_apply_patch_multi_file_mixed` — V4A with Update + Add + Delete on 3 different files. Assert all 3 committed
- [ ] **Step 4.5:** `test_apply_patch_path_traversal_blocked` — V4A with Update on `../../etc/passwd`. Assert error returned, filesystem untouched
- [ ] **Step 4.6:** `test_apply_patch_move_target_outside_workspace_blocked` — V4A with Update + Move to `../escape.txt`. Assert error returned, source file unchanged
- [ ] **Step 4.7:** Run `cargo test -p synthia-tool apply_patch::` — all 7 tests pass

**Commit:** `test(tool): add apply_patch integration tests for atomic multi-file scenarios`

---

## Task 5: Tool Registry integration

**Files:** `crates/synthia-tool/src/registry/registration.rs`, `crates/synthia-tool/src/builtin/mod.rs`

- [ ] **Step 5.1:** In `builtin/mod.rs` add `pub mod apply_patch;` and `pub mod v4a;` at the top of the builtin module list, and `pub use apply_patch::ApplyPatchTool;` in the re-export section
- [ ] **Step 5.2:** In `registration.rs` `register_defaults()` add `registry.register(ToolEntry::new(Arc::new(ApplyPatchTool::default())));` after the `MultiEditTool` line
- [ ] **Step 5.3:** Add `#[test] fn test_register_defaults_includes_apply_patch()` in `registration.rs` tests that builds a default registry and asserts `apply_patch` is present
- [ ] **Step 5.4:** Run `cargo test -p synthia-tool registry::` — test passes
- [ ] **Step 5.5:** Run `cargo build -p synthia-agent` — confirms no consumer breakage from new tool

**Commit:** `feat(tool): register apply_patch in default tool registry`

---

## Task 6: Formatting, lint, and downstream verification

- [ ] **Step 6.1:** `cargo +nightly fmt --all` — confirm no diff
- [ ] **Step 6.2:** `cargo clippy --all-targets --all-features --tests -p synthia-tool -- -D warnings` — fix any new warnings
- [ ] **Step 6.3:** `cargo test -p synthia-tool` — full test suite passes (14 new tests + existing)
- [ ] **Step 6.4:** `cargo test -p synthia-agent` — confirms no downstream breakage (registry changes are additive)
- [ ] **Step 6.5:** `cargo test -p synthia-cli` — confirms CLI tool listing works with new tool
- [ ] **Step 6.6:** `openspec validate apply-patch-tool --strict` — confirms all OpenSpec artifacts pass validation
- [ ] **Step 6.7:** Update project memory: add lesson learned if any new pattern emerges (e.g., tool extraction pattern)

**Commit:** (no separate commit — these are verification steps; the work is already committed across tasks 1-5)

---

## Verification Checklist

Before marking the change complete, confirm:
- [ ] All 5 OpenSpec artifacts (brainstorm.md, design.md, proposal.md, spec delta, tasks.md) exist and pass `openspec validate`
- [ ] 14 new tests pass (6 parser unit tests + 7 tool integration tests + 1 registry test)
- [ ] `cargo clippy -p synthia-tool -- -D warnings` is clean
- [ ] `cargo test -p synthia-agent` still passes (no consumer breakage)
- [ ] Final git log shows clean conventional-commits messages scoped to `feat(tool):` and `test(tool):`
