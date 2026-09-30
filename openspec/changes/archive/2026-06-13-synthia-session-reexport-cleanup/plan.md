# synthia-session-reexport-cleanup Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development
> to implement this plan task-by-task. The change is small enough
> (~310 lines added across 3 files + 8 files deleted) that a single
> worker can complete it in one sitting.

**Goal:** Eliminate the dual-`Session` / dual-`SessionManager`
name-shadowing trap in `synthia_session::lib.rs` and add a
three-layer guard (doc tests, integration test, CI script) to
prevent re-introduction.

**Architecture:**
- **Layer 1 (compile_fail doc tests)**: 3 `compile_fail` snippets in
  `lib.rs` pin the FORBIDDEN patterns; 3 positive snippets pin the
  CANONICAL paths and stable re-exports.
- **Layer 2 (integration test)**: 6 tests in
  `tests/reexport_policy.rs` cover type-level invariants, canonical
  path drift, and structural drift in the policy block.
- **Layer 3 (CI shell script)**: 5 grep/awk checks in
  `scripts/check_reexports.sh` cover structural drift in `lib.rs`
  and unqualified imports across the workspace.

**Tech Stack:** Rust (stable), bash, awk, grep. No new dependencies.

**Reference:**
- `proposal.md` — the "why" and the user-visible impact
- `design.md` — the root cause analysis and the fix design
- `specs/synthia-session-reexport-policy/spec.md` — 5 ADDED Requirements
- `tasks.md` — 8 task groups, 34 micro-tasks

---

## Task 1: Policy Documentation

- [x] **Step 1:** Open `crates/synthia-session/src/lib.rs`
- [x] **Step 2:** Add a 30-line comment block at the top of `lib.rs`
  with the re-export policy
- [x] **Step 3:** List the 3 known-conflict names: `Session`,
  `SessionManager`, `SessionError`
- [x] **Step 4:** Document the canonical import paths
- [x] **Step 5:** State the rule: "do NOT add a new `pub use
  module::Foo` for any type that exists in more than one module"

## Task 2: Remove Conflicting Re-export

- [x] **Step 1:** Delete the line
  `pub use session::{Session, SessionError, SessionManager};` from
  `lib.rs`
- [x] **Step 2:** Run `cargo check -p synthia-session` — must pass

## Task 3: Update Live Consumers

- [x] **Step 1:** Open `crates/synthia-agent/src/error/mod.rs`
- [x] **Step 2:** Change `From<synthia_session::SessionError>` to
  `From<synthia_session::session::SessionError>`
- [x] **Step 3:** Open
  `crates/synthia-server/tests/e2e_server_sse_test.rs` and
  `crates/synthia-server/tests/e2e_server_ws_test.rs`
- [x] **Step 4:** Change `use synthia_session::SessionManager;` to
  `use synthia_session::manager::SessionManager;` in both files
- [x] **Step 5:** Grep workspace for any other live consumers of
  `synthia_session::SessionManager` or
  `synthia_session::SessionError` — must find 0 results

## Task 4: Delete Dead Code

- [x] **Step 1:** Delete
  `crates/synthia-memory/src/memory_pipeline.rs` (279 lines)
- [x] **Step 2:** Delete the entire
  `crates/synthia-memory/src/memory_pipeline/` directory (6
  submodules, ~2342 lines)
- [x] **Step 3:** Delete
  `crates/synthia-cli/src/scheduler/mod.rs` (124 lines)
- [x] **Step 4:** Delete empty parent directories
- [x] **Step 5:** Run `cargo check --workspace --tests` — must be
  clean

## Task 5: Layer 1 Doc Tests

- [x] **Step 1:** Open `crates/synthia-session/src/lib.rs`
- [x] **Step 2:** Add `_doc_session_manager_forbidden` function
  with `compile_fail` doc test
- [x] **Step 3:** Add `_doc_session_error_forbidden` function
  with `compile_fail` doc test
- [x] **Step 4:** Add `_doc_historical_offender` function with
  `compile_fail` doc test
- [x] **Step 5:** Add `_doc_legacy_session_canonical` function
  with positive doc test
- [x] **Step 6:** Add `_doc_canonical_paths` function with
  positive doc test
- [x] **Step 7:** Add `_doc_stable_reexports` function with
  positive doc test
- [x] **Step 8:** Run `cargo test -p synthia-session --doc` —
  all 6 doc tests pass

## Task 6: Layer 2 Integration Test

- [x] **Step 1:** Create
  `crates/synthia-session/tests/reexport_policy.rs`
- [x] **Step 2:** Add
  `test_crate_root_session_is_state_machine_record` — confirms
  `Session` at root is the state-machine record
- [x] **Step 3:** Add
  `test_legacy_session_record_is_qualified` — confirms
  `session::Session` is reachable and distinct
- [x] **Step 4:** Add `test_session_manager_qualified_paths` —
  confirms `manager::SessionManager` (struct) and
  `session::SessionManager` (trait) are both reachable
- [x] **Step 5:** Add `test_session_error_qualified_path` —
  confirms `session::SessionError` is reachable
- [x] **Step 6:** Add `test_stable_reexports_at_crate_root` —
  smoke test for stable re-exports
- [x] **Step 7:** Add `test_lib_rs_documents_policy` — asserts
  the policy block is present and the historical offender is
  absent
- [x] **Step 8:** Run
  `cargo test -p synthia-session --test reexport_policy` — all
  6 tests pass
- [x] **Step 9:** Mutation test: re-introduce the historical
  offender in `lib.rs`, confirm
  `test_lib_rs_documents_policy` fails to compile, then revert

## Task 7: Layer 3 CI Shell Script

- [x] **Step 1:** Create `scripts/check_reexports.sh`
- [x] **Step 2:** Add `chmod +x scripts/check_reexports.sh`
- [x] **Step 3:** Implement Check 1: historical offender absent
- [x] **Step 4:** Implement Check 2: required module re-exports
  present
- [x] **Step 5:** Implement Check 3: policy header documents the
  3 conflict names
- [x] **Step 6:** Implement Check 4: integration test file exists
- [x] **Step 7:** Implement Check 5: workspace uses qualified paths
- [x] **Step 8:** Run `bash scripts/check_reexports.sh` — all 5
  checks pass
- [x] **Step 9:** Mutation test: add a violating file, confirm
  the script exits 1, then remove the file

## Task 8: Verification

- [x] **Step 1:** Run `cargo +nightly fmt --all` — must be clean
- [x] **Step 2:** Run
  `cargo clippy -p synthia-session --all-targets --all-features --tests`
  — 0 new warnings
- [x] **Step 3:** Run `cargo test -p synthia-session --tests` —
  all tests pass
- [x] **Step 4:** Run `cargo test -p synthia-session --doc` —
  all 6 doc tests pass
- [x] **Step 5:** Run `bash scripts/check_reexports.sh` — all 5
  checks pass
- [x] **Step 6:** Run
  `cargo test -p synthia-agent --test explicit_recovery_paths_test` —
  8 tests pass
- [x] **Step 7:** Run
  `cargo test -p synthia-server --tests --no-fail-fast` — 0
  failures
- [x] **Step 8:** Run `cargo check --workspace --tests` — must
  be clean
