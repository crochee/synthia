## 1. Policy Documentation

- [x] 1.1 Add 30-line re-export policy block to `crates/synthia-session/src/lib.rs`
- [x] 1.2 Document the 3 known-conflict names (`Session`, `SessionManager`, `SessionError`)
- [x] 1.3 Document the canonical import paths for each conflict name
- [x] 1.4 Document the rule "do not add a new `pub use module::Foo` for any type that exists in more than one module"

## 2. Remove Conflicting Re-export

- [x] 2.1 Delete the explicit re-export line in `lib.rs`:
  `pub use session::{Session, SessionError, SessionManager};`
- [x] 2.2 Run `cargo check -p synthia-session` — must pass

## 3. Update Live Consumers

- [x] 3.1 Update `crates/synthia-agent/src/error/mod.rs`:
  change `From<synthia_session::SessionError>` to
  `From<synthia_session::session::SessionError>`
- [x] 3.2 Update `crates/synthia-server/tests/e2e_server_sse_test.rs`:
  change `use synthia_session::SessionManager;` to
  `use synthia_session::manager::SessionManager;`
- [x] 3.3 Update `crates/synthia-server/tests/e2e_server_ws_test.rs`:
  same as 3.2
- [x] 3.4 Grep workspace for any other live consumers of
  `synthia_session::SessionManager` or
  `synthia_session::SessionError` — must find 0 results

## 4. Delete Dead Code

- [x] 4.1 Delete `crates/synthia-memory/src/memory_pipeline.rs` (279 lines)
- [x] 4.2 Delete `crates/synthia-memory/src/memory_pipeline/` directory
  (6 submodules, ~2342 lines)
- [x] 4.3 Delete `crates/synthia-cli/src/scheduler/mod.rs` (124 lines)
- [x] 4.4 Delete empty parent directories (`memory_pipeline/` and
  `scheduler/`)
- [x] 4.5 Verify `cargo check --workspace --tests` is clean (no
  reference to the deleted files)

## 5. Layer 1: Doc Tests

- [x] 5.1 Add 3 `compile_fail` doc tests for the forbidden patterns
  (`SessionManager`, `SessionError`, historical offender)
- [x] 5.2 Add 1 positive doc test for the canonical qualified paths
- [x] 5.3 Add 1 positive doc test for the stable crate-root re-exports
- [x] 5.4 Add 1 positive doc test for the legacy `Session` qualified path
- [x] 5.5 Run `cargo test -p synthia-session --doc` — all 6 doc tests pass

## 6. Layer 2: Integration Test

- [x] 6.1 Create `crates/synthia-session/tests/reexport_policy.rs`
- [x] 6.2 Test: `test_crate_root_session_is_state_machine_record` —
  confirms `Session` at root is the state-machine record
- [x] 6.3 Test: `test_legacy_session_record_is_qualified` —
  confirms `session::Session` is reachable and distinct
- [x] 6.4 Test: `test_session_manager_qualified_paths` —
  confirms `manager::SessionManager` (struct) and
  `session::SessionManager` (trait) are both reachable
- [x] 6.5 Test: `test_session_error_qualified_path` — confirms
  `session::SessionError` is reachable
- [x] 6.6 Test: `test_stable_reexports_at_crate_root` — smoke test
  for stable re-exports
- [x] 6.7 Test: `test_lib_rs_documents_policy` — asserts the policy
  block is present and the historical offender is absent
- [x] 6.8 Run `cargo test -p synthia-session --test reexport_policy` —
  all 6 tests pass
- [x] 6.9 Mutate `lib.rs` to re-introduce the historical offender;
  confirm `test_lib_rs_documents_policy` fails to compile

## 7. Layer 3: CI Shell Script

- [x] 7.1 Create `scripts/check_reexports.sh` (executable, +x)
- [x] 7.2 Check 1: historical offender absent
- [x] 7.3 Check 2: required module re-exports present
- [x] 7.4 Check 3: policy header documents the 3 conflict names
- [x] 7.5 Check 4: integration test `reexport_policy.rs` exists
- [x] 7.6 Check 5: workspace uses qualified paths
- [x] 7.7 Run the script — all 5 checks pass
- [x] 7.8 Mutate the workspace with a violating file; confirm the
  script returns exit 1

## 8. Verification

- [x] 8.1 Run `cargo +nightly fmt --all` — must be clean
- [x] 8.2 Run `cargo clippy -p synthia-session --all-targets --all-features --tests` — 0 new warnings (1 pre-existing `items_after_test_module` warning in `manager.rs:607` remains, unchanged)
- [x] 8.3 Run `cargo test -p synthia-session --tests` — all tests pass
- [x] 8.4 Run `cargo test -p synthia-session --doc` — all doc tests pass
- [x] 8.5 Run `bash scripts/check_reexports.sh` — all 5 checks pass
- [x] 8.6 Run `cargo test -p synthia-agent --test explicit_recovery_paths_test` — all 8 tests pass
- [x] 8.7 Run `cargo test -p synthia-server --tests --no-fail-fast` — no failures
- [x] 8.8 Run `cargo check --workspace --tests` — must be clean
