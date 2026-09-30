# Error Recovery Cascade — Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development
> to implement this plan task-by-task.

**Goal:** Wire the existing L1-L5 error recovery framework into StreamBuilder's tool execution path.

**Architecture:** L1/L2 in ToolExecutor (truncate + retry); L3/L4/L5 in new `recovery_cascade.rs` (fallback + compact + reset); StreamBuilder calls cascade on tool error.

**Tech Stack:** Rust, tokio, `synthia-context::Compactor`, `synthia-guardian::LoopDetectorSet`

---

## Task 1: L1 Truncate (tool-output-truncate)

- [ ] **Step 1:** Read `crates/synthia-agent/src/types.rs` — locate `ToolOutput` struct
- [ ] **Step 2:** Add `truncated: bool` and `original_len: usize` fields to `ToolOutput`
- [ ] **Step 3:** Add `truncate_if_large(output: &str, threshold: usize) -> (String, bool, usize)` to `error_recovery/mod.rs`
- [ ] **Step 4:** Read `crates/synthia-agent/src/stream_builder/steps/tool_execute.rs` — find where tool output is returned
- [ ] **Step 5:** After tool execution, call `truncate_if_large()` if output > 16KB, set fields on result
- [ ] **Step 6:** `cargo test -p synthia-agent --lib error_recovery::tests` — verify truncation tests pass

## Task 2: L2 Retry (tool-retry)

- [ ] **Step 1:** Add `is_retryable(error: &str) -> bool` to `error_recovery/retry.rs`
- [ ] **Step 2:** Add `calculate_backoff(attempt: u32) -> Duration` (2 * 2^attempt seconds)
- [ ] **Step 3:** Modify `ToolExecutor::execute()` to wrap execution in retry loop: on error, if `is_retryable()`, sleep backoff, retry up to 2 times
- [ ] **Step 4:** Add unit tests in `error_recovery/retry.rs::tests`
- [ ] **Step 5:** `cargo test -p synthia-agent --lib error_recovery::retry::tests` — verify retry tests pass

## Task 3: L3 Fallback (tool-fallback)

- [ ] **Step 1:** Read `error_recovery/fallback.rs` — verify `FallbackProvider` covers all tools
- [ ] **Step 2:** Add consecutive failure tracking: `HashMap<String, u32>` in `ErrorRecoveryCoordinator`
- [ ] **Step 3:** Create `crates/synthia-agent/src/stream_builder/steps/recovery_cascade.rs` with `run_recovery_cascade()` function
- [ ] **Step 4:** Implement L3: if tool failed 2x consecutively → `FallbackProvider::get_fallback()` → return message as `ToolResult`
- [ ] **Step 5:** Read `stream_builder/builder.rs` line ~300-400 — find tool step error handler
- [ ] **Step 6:** Wire `run_recovery_cascade()` into tool step error path
- [ ] **Step 7:** Add unit tests for consecutive failure detection and fallback return
- [ ] **Step 8:** `cargo test -p synthia-agent --lib error_recovery::fallback::tests` — verify fallback tests pass

## Task 4: L4 Auto-Compact (auto-compact-on-error)

- [ ] **Step 1:** Implement L4 in `recovery_cascade.rs`: after L3 escalation, check `ctx.token_ratio()`
- [ ] **Step 2:** If ratio > 0.8 → call `compact_with_fallback()` from `synthia_context`
- [ ] **Step 3:** On success → `record_success()`, return `Recovered`
- [ ] **Step 4:** On failure → escalate to L5
- [ ] **Step 5:** Add unit tests: high ratio triggers compact, low ratio skips compact
- [ ] **Step 6:** `cargo test -p synthia-agent --lib recovery_cascade::tests` — verify compact tests pass

## Task 5: L5 Reset (session-reset)

- [ ] **Step 1:** Read `error_recovery/reset.rs` — locate `ResetCoordinator::execute()`
- [ ] **Step 2:** Implement `ResetScope::Conversation`: clear `ctx.messages`, preserve session_id, reset error counter
- [ ] **Step 3:** Call `loop_detector.reset()` in L5 branch
- [ ] **Step 4:** Drain steering channel in L5 branch
- [ ] **Step 5:** Implement L5 in `recovery_cascade.rs`: call `ResetCoordinator::execute(Conversation)`
- [ ] **Step 6:** On reset success → `record_success()`, return `Recovered`
- [ ] **Step 7:** On reset failure → return `FailFast`, store cooldown timestamp
- [ ] **Step 8:** Add unit tests: reset clears context, loop detector resets
- [ ] **Step 9:** `cargo test -p synthia-agent --lib error_recovery::reset::tests` — verify reset tests pass

## Task 6: Integration & Full Verification

- [ ] **Step 1:** `cargo build -p synthia-agent` — verify compilation
- [ ] **Step 2:** `cargo test -p synthia-agent --lib` — all agent tests pass
- [ ] **Step 3:** `cargo clippy --all-targets --all-features --tests --all` — no new warnings
- [ ] **Step 4:** `cargo test --workspace` — full workspace tests pass
- [ ] **Step 5:** Run e2e smoke test: `cargo test -p synthia-e2e`
- [ ] **Step 6:** Commit: `feat(agent): error recovery cascade L1-L5`
