# Build Complete E2E, UT, and Benchmarks Implementation Plan

> **For agentic workers:** Use the Task tool to implement this plan
> task-by-task, verifying each step with `cargo test` and `cargo clippy`.

**Goal:** Establish comprehensive test coverage and a criterion-based
benchmark harness for Synthia without modifying production code behavior.

**Architecture:** Organize tests by naming convention (e2e/unit/integration)
and place benchmarks co-located with the `synthia-agent` crate using
`criterion` for statistical rigor. E2E tests use the existing
`test-support` crate for mocks and run against a real (but isolated)
session directory.

**Tech Stack:** Rust, criterion, cargo test, test-support crate,
tempfile for test isolation.

---

## Task 1: Test coverage standards

- [ ] **Step 1:** Create `crates/synthia-agent/tests/README.md` with
  sections: "Test Organization", "Naming Conventions", "Run Commands".
- [ ] **Step 2:** List all existing e2e tests in `tests/` and verify
  naming; flag any that don't follow `e2e_<scenario>_test.rs`.
- [ ] **Step 3:** Run `cargo test -p synthia-session -- --list` and
  `cargo test -p synthia-tool -- --list` to audit existing test coverage.
- [ ] **Step 4:** Create `crates/synthia-tool/src/tool_test.rs` with
  unit tests for the core tool execution path.
- [ ] **Step 5:** Create `crates/synthia-tool/src/permission_test.rs`
  with unit tests for permission checking.
- [ ] **Step 6:** Create `crates/synthia-memory/src/memory_test.rs`
  with unit tests for the hot/cold memory interface.
- [ ] **Step 7:** Create `crates/synthia-context/src/context_test.rs`
  with unit tests for context assembly.
- [ ] **Step 8:** Create `crates/synthia-session/src/integration/store_integration_test.rs`
  and `crates/synthia-session/src/integration/session_lifecycle_integration_test.rs`.
- [ ] **Step 9:** Run `cargo test --workspace` to verify all new tests pass.
- [ ] **Step 10:** Commit: "test: add unit and integration tests for uncovered modules".

---

## Task 2: Benchmark harness

- [ ] **Step 1:** Create `crates/synthia-agent/benches/` directory and
  `Cargo.toml` with `criterion` dependency and `[[bench]]` targets.
- [ ] **Step 2:** Create `crates/synthia-agent/benches/lib.rs` that
  declares the module structure (mod loop, mod session, mod event_writer).
- [ ] **Step 3:** Implement `crates/synthia-agent/benches/loop.rs`:
  `criterion::Benchmark::new("agent_loop_latency", |bencher| ...)`
  that creates a minimal agent loop and measures a single turn.
- [ ] **Step 4:** Implement warm-up in `loop.rs`: run the benchmarked
  code in a loop for 3 seconds before `bencher.iter(...)`.
- [ ] **Step 5:** Implement `crates/synthia-agent/benches/session.rs`:
  benchmark session creation throughput using a mock SessionStore.
- [ ] **Step 6:** Implement `crates/synthia-agent/benches/event_writer.rs`:
  benchmark JSONL event append throughput using a temp file.
- [ ] **Step 7:** Create `crates/synthia-agent/benches/README.md`
  documenting each benchmark group, what it measures, and the run
  command (`cargo bench --package synthia-agent`).
- [ ] **Step 8:** Run `cargo bench --package synthia-agent -- --verbose`
  and confirm all 3 benchmark groups produce statistical output.
- [ ] **Step 9:** Commit: "bench: add criterion benchmark harness to synthia-agent".

---

## Task 3: E2E test suite additions

- [ ] **Step 1:** Create `crates/synthia-agent/tests/e2e_multi_turn_conversation_test.rs`.
  Use `test_support::mock::MockLlm` to return 3 pre-scripted responses.
  Assert conversation context is preserved across all 3 turns.
- [ ] **Step 2:** Create `crates/synthia-agent/tests/e2e_session_pause_resume_test.rs`.
  Start a session, pause after first turn, then resume. Assert the
  second turn continues from correct state.
- [ ] **Step 3:** Create `crates/synthia-agent/tests/e2e_tool_call_sequence_test.rs`.
  Script the LLM to return sequential tool calls (read_file then bash).
  Assert each tool result is incorporated into the next LLM response.
- [ ] **Step 4:** Create `crates/synthia-agent/tests/e2e_guardian_permission_gate_test.rs`.
  Configure guardian to block a specific tool. Assert the agent
  receives a clear permission-denied message and does not crash.
- [ ] **Step 5:** Create `crates/synthia-agent/tests/e2e_session_teardown_test.rs`.
  Run a 2-turn session to completion. Assert the session JSONL log
  contains all expected events and the temp directory is cleaned up.
- [ ] **Step 6:** Verify each new test uses a unique temp directory
  via `tempfile::TempDir` and cleans up after itself.
- [ ] **Step 7:** Run `cargo test -p synthia-agent e2e_` and confirm
  all 5 new e2e tests pass.
- [ ] **Step 8:** Commit: "test: add 5 new e2e tests for session pause/resume, tool sequences, guardian gate, and teardown".

---

## Task 4: Workspace-level test documentation

- [ ] **Step 1:** Create `tests/README.md` at workspace root with
  sections: "Test Categories", "Naming Conventions", "Run Commands",
  "Coverage Expectations".
- [ ] **Step 2:** Document `cargo test --workspace` for full test run.
- [ ] **Step 3:** Document `cargo bench --package synthia-agent` for
  benchmarks.
- [ ] **Step 4:** Document CI expectations: tests run on every PR,
  benchmarks are informational.
- [ ] **Step 5:** Commit: "docs: add tests/README.md documenting coverage standards".

---

## Task 5: Verification

- [ ] **Step 1:** Run `cargo check --all-targets --all-features`
  across all affected crates.
- [ ] **Step 2:** Run `cargo test --workspace` and confirm 0 failures.
- [ ] **Step 3:** Run `cargo clippy --all-targets --all-features
  --tests --all -- -D warnings` and fix all warnings.
- [ ] **Step 4:** Run `cargo +nightly fmt --all`.
- [ ] **Step 5:** Run `git diff --stat` to confirm only test/bench/doc
  files were added, no production source modified.
- [ ] **Step 6:** Commit: "chore: final lint, fmt, and verification".
