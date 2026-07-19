# Error Recovery Cascade — Tasks

## 1. Phase 1: L1 Truncate (tool-output-truncate)

- [ ] 1.1 Add `truncated: bool` and `original_len: usize` fields to `ToolOutput` struct in `crates/synthia-agent/src/types.rs`
- [ ] 1.2 Add `truncate_if_large(output: &str, threshold: usize) -> (String, bool, usize)` helper function
- [ ] 1.3 Integrate truncation into `ToolExecutor::execute()` — check output > 16KB after execution, truncate if needed, set `ToolOutput.truncated = true`
- [ ] 1.4 Write unit tests: output exactly 16KB not truncated, output 50KB truncated to head+tail+marker, output < 16KB not truncated
- [ ] 1.5 Write unit tests: `ToolOutput` carries correct `truncated` and `original_len` values

## 2. Phase 2: L2 Retry (tool-retry)

- [ ] 2.1 Add `is_retryable(error: &str) -> bool` function matching "timeout", "timed out", "connection reset", "temporary failure", "rate limit", "503", "502", "429", "SSL", "DNS"
- [ ] 2.2 Add `calculate_backoff(attempt: u32) -> Duration` using formula: 2 * 2^attempt seconds
- [ ] 2.3 Integrate retry loop into `ToolExecutor::execute()` — on error, check `is_retryable()`, sleep backoff, retry up to 2 times
- [ ] 2.4 Write unit tests: timeout error triggers retry, rate limit triggers retry, non-retryable error does not retry
- [ ] 2.5 Write unit tests: exponential backoff 2s → 4s → 8s progression
- [ ] 2.6 Write unit tests: success on second attempt stops retries, all 3 attempts fail propagates failure

## 3. Phase 3: L3 Fallback (tool-fallback)

- [ ] 3.1 Review existing `FallbackProvider::get_fallback()` in `error_recovery/fallback.rs` — verify all tool fallbacks (web_fetch, bash, subagent, mcp_tool, file_read)
- [ ] 3.2 Add consecutive failure tracking to `ErrorRecoveryCoordinator`: `consecutive_tool_failures: HashMap<ToolName, u32>`
- [ ] 3.3 Create `crates/synthia-agent/src/stream_builder/steps/recovery_cascade.rs` with `run_recovery_cascade(error, tool_name, ctx) -> RecoveryAction` function
- [ ] 3.4 Implement L3 branch in `run_recovery_cascade()`: check if same tool failed 2x → call `FallbackProvider::get_fallback()` → return fallback message
- [ ] 3.5 Wire `run_recovery_cascade()` into StreamBuilder tool step error handler in `builder.rs`
- [ ] 3.6 Write unit tests: same tool fails twice triggers fallback, different tool failures don't count as consecutive
- [ ] 3.7 Write unit tests: fallback resets error counter via `record_success()`
- [ ] 3.8 Write unit tests: tool with no fallback escalates to L4

## 4. Phase 4: L4 Auto-Compact (auto-compact-on-error)

- [ ] 4.1 Implement L4 branch in `run_recovery_cascade()`: after L3 escalation, check `ctx.token_ratio() > 0.8`
- [ ] 4.2 If ratio > 80%, call `compact_with_fallback()` with current messages and budget
- [ ] 4.3 On compact success → call `record_success()`, return `RecoveryAction::Recovered`
- [ ] 4.4 On compact failure → escalate to L5
- [ ] 4.5 If ratio <= 80%, skip compact, escalate directly to L5
- [ ] 4.6 Write unit tests: high context ratio triggers auto-compact, low context ratio skips compact
- [ ] 4.7 Write integration test: L3 fails → L4 compact succeeds → session continues

## 5. Phase 5: L5 Reset (session-reset)

- [ ] 5.1 Implement `ResetCoordinator::execute(scope: ResetScope) -> ResetResult` — initially implement `Conversation` scope
- [ ] 5.2 Implement `ResetScope::Conversation`: discard `ctx.messages`, preserve session ID and HotMemory, reset consecutive error counter
- [ ] 5.3 Call `LoopDetectorSet::reset()` after L5 reset
- [ ] 5.4 Drain steering channel after L5 reset
- [ ] 5.5 Wire L5 into `run_recovery_cascade()` — after L4 compact failure, call `ResetCoordinator::execute(Conversation)`
- [ ] 5.6 On reset success → call `record_success()`, return `RecoveryAction::Recovered`
- [ ] 5.7 On reset failure → return `RecoveryAction::FailFast`, start 30s cooldown
- [ ] 5.8 Write unit tests: reset discards context, preserves session metadata
- [ ] 5.9 Write unit tests: loop detector state is cleared after reset
- [ ] 5.10 Write unit tests: reset failure enters cooldown

## 6. Phase 6: Integration & Verification

- [ ] 6.1 Run `cargo test -p synthia-agent --lib` — all existing tests pass
- [ ] 6.2 Run `cargo clippy --all-targets --all-features --tests --all` — no new warnings
- [ ] 6.3 Run `cargo test -p synthia-context` — ensure compactor integration works
- [ ] 6.4 Run `cargo test -p synthia-guardian` — ensure loop detector reset works
- [ ] 6.5 Run full workspace test suite: `cargo test --workspace`
- [ ] 6.6 Verify 3 recovery cycles → fail-fast behavior
- [ ] 6.7 Verify L5 cooldown prevents immediate retry
