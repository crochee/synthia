# Verify: synthia-session-reexport-cleanup

Date: 2026-06-13

## Layer 1 — compile_fail doc tests

Command: `cargo test -p synthia-session --doc`

Result: 6 doc tests pass
- `_doc_session_manager_forbidden` (compile_fail) — passes
- `_doc_session_error_forbidden` (compile_fail) — passes
- `_doc_historical_offender` (compile_fail) — passes
- `_doc_legacy_session_canonical` (positive) — passes
- `_doc_canonical_paths` (positive) — passes
- `_doc_stable_reexports` (positive) — passes

## Layer 2 — integration test

Command: `cargo test -p synthia-session --test reexport_policy`

Result: 6 tests pass
- `test_crate_root_session_is_state_machine_record` — passes
- `test_legacy_session_record_is_qualified` — passes
- `test_session_manager_qualified_paths` — passes
- `test_session_error_qualified_path` — passes
- `test_stable_reexports_at_crate_root` — passes
- `test_lib_rs_documents_policy` — passes

## Layer 3 — CI shell script

Command: `bash scripts/check_reexports.sh`

Result: 5 checks pass
1. Historical offender absent — OK
2. Required module re-exports present — OK
3. Policy header documents the 3 conflict names — OK
4. Integration test `reexport_policy.rs` exists — OK
5. Workspace uses qualified paths — OK

## Mutation tests (negative cases)

To verify the layers actually catch violations, three mutation
tests were run and then reverted:

### Mutation 1: re-introduce the historical offender

Added `pub use session::{Session, SessionError, SessionManager};`
to `lib.rs`.

Expected: `test_lib_rs_documents_policy` fails to compile because
`Session::new("test-id".to_string())` no longer resolves (the
crate-root `Session` becomes the legacy conversation record which
has no `new` method).

Actual: as expected. Layer 2 catches the bug.

After verifying, the mutation was reverted.

### Mutation 2: add a violating unqualified import

Created `crates/synthia-server/tests/_bad_test.rs` containing
`use synthia_session::SessionManager;`.

Expected: `bash scripts/check_reexports.sh` exits 1 with the
violating file path in the error message.

Actual: as expected. Layer 3 catches the bug.

After verifying, the mutation was reverted.

## Workspace verification

Command: `cargo check --workspace --tests`

Result: 0 errors, 0 new warnings. The only remaining warning is a
pre-existing `items_after_test_module` in
`crates/synthia-session/src/manager.rs:607` that is unchanged from
master.

## Downstream impact

Command: `cargo test -p synthia-agent --test explicit_recovery_paths_test`

Result: 8 tests pass (no regression).

Command: `cargo test -p synthia-server --tests --no-fail-fast`

Result: 0 failures (no regression).

## Pre-existing failures (unchanged baseline)

The following 4 failures exist on master and are NOT caused by
this change:

- `test_multi_turn_memory_with_tracking_provider` (synthia-agent)
- `test_react_loop_emits_llm_deltas` (synthia-agent)
- `test_react_loop_respects_max_iterations` (synthia-agent)
- 9 doc tests in `synthia_agent::error::AgentError::*` (synthia-agent)

These are tracked in `project_memory.md` ("Premature trait
abstraction..." and "synthia-session has pre-existing dual
Session..." lessons learned) and are out of scope for this
surgical change.
