## 1. Test coverage standards

- [ ] 1.1 Create `crates/synthia-agent/tests/README.md` documenting test organization and naming conventions.
- [ ] 1.2 Verify existing e2e tests follow the `e2e_<scenario>_test.rs` naming pattern; rename any that don't.
- [ ] 1.3 Audit `synthia-session`, `synthia-tool`, `synthia-memory`, `synthia-context` for test coverage gaps.
- [ ] 1.4 Add at least 2 unit test files to `synthia-tool` for uncovered modules.
- [ ] 1.5 Add at least 1 unit test file to `synthia-memory` for uncovered modules.
- [ ] 1.6 Add at least 1 unit test file to `synthia-context` for uncovered modules.
- [ ] 1.7 Ensure `synthia-session` has at least 2 integration test files.

## 2. Benchmark harness

- [ ] 2.1 Add `crates/synthia-agent/benches/` directory with `Cargo.toml` declaring `[[bench]]` targets.
- [ ] 2.2 Create `crates/synthia-agent/benches/lib.rs` re-exporting all benchmarks.
- [ ] 2.3 Implement `crates/synthia-agent/benches/loop.rs` with agent loop latency benchmark (criterion).
- [ ] 2.4 Implement `crates/synthia-agent/benches/session.rs` with session creation throughput benchmark.
- [ ] 2.5 Implement `crates/synthia-agent/benches/event_writer.rs` with JSONL event append throughput benchmark.
- [ ] 2.6 Add warm-up phase (3s) and statistical output (mean, median, std dev, min, max) to each benchmark.
- [ ] 2.7 Create `crates/synthia-agent/benches/README.md` documenting benchmark categories and run commands.

## 3. E2E test suite additions

- [ ] 3.1 Create `crates/synthia-agent/tests/e2e_multi_turn_conversation_test.rs` covering 3-turn conversation context preservation.
- [ ] 3.2 Create `crates/synthia-agent/tests/e2e_session_pause_resume_test.rs` covering pause mid-turn and resume.
- [ ] 3.3 Create `crates/synthia-agent/tests/e2e_tool_call_sequence_test.rs` covering sequential tool calls with result incorporation.
- [ ] 3.4 Create `crates/synthia-agent/tests/e2e_guardian_permission_gate_test.rs` covering guardian blocking behavior.
- [ ] 3.5 Create `crates/synthia-agent/tests/e2e_session_teardown_test.rs` covering clean session end and event flush.
- [ ] 3.6 Verify all new e2e tests use `test_support` crate for mocks and run in isolation with unique temp session dirs.
- [ ] 3.7 Run `cargo test -p synthia-agent` and confirm all e2e tests pass.

## 4. Workspace-level test documentation

- [ ] 4.1 Create `tests/README.md` at workspace root documenting coverage categories, naming conventions, and run commands.
- [ ] 4.2 Document benchmark run commands: `cargo bench --package synthia-agent`.
- [ ] 4.3 Document CI expectations for test and benchmark runs.

## 5. Verification

- [ ] 5.1 Run `cargo check --all-targets --all-features` for all affected crates.
- [ ] 5.2 Run `cargo test --workspace` and confirm all tests pass.
- [ ] 5.3 Run `cargo clippy --all-targets --all-features --tests --all -- -D warnings` and fix any warnings.
- [ ] 5.4 Run `cargo +nightly fmt --all`.
- [ ] 5.5 Confirm no production source files were modified (only tests, benches, and docs added).
