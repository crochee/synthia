## 1. AgentEvent Schema Extension

- [x] 1.1 Add `AgentEvent::RecoveryApplied { level_number: u32, tool_name: Option<String>, message: String, iteration: usize }` variant in `crates/synthia-agent/src/events.rs`
- [x] 1.2 Update `AgentEvent` doc comment to mention the new variant
- [x] 1.3 Add a unit test verifying the variant can be constructed and matched

## 2. BuilderSteps Cascade State

- [x] 2.1 Add `reset: ResetCoordinator` field to `BuilderSteps` in `stream_builder/builder.rs`
- [x] 2.2 Add `failure_tracker: ConsecutiveFailureTracker` field to `BuilderSteps`
- [x] 2.3 Initialize the two new fields in `BuilderSteps::new` (`ResetCoordinator::new()` and `ConsecutiveFailureTracker::new()`)
- [x] 2.4 Run `cargo check -p synthia-agent` — must pass with the new fields

## 3. L1 Truncation at Tool Result Boundary

- [x] 3.1 Write failing integration test: oversized tool result (`>30KB`) is truncated and `RecoveryApplied { level_number: 1, ... }` is yielded
- [x] 3.2 In `builder.rs`, add `truncate_output` call inside the `for result in &tool_results` loop
- [x] 3.3 Yield `AgentEvent::RecoveryApplied { level_number: 1, tool_name: Some(...), message: "Truncated tool output (N -> M bytes)", iteration }` when truncated
- [x] 3.4 Write integration test: small tool result passes through byte-identical, no `RecoveryApplied` event

## 4. LLM Sampling Error Path → Cascade

- [x] 4.1 Write failing integration test: LLM sampling error → cascade runs L3-L5 → either `Recovered` continues iteration or `FailFast` yields `SessionEnded`
- [x] 4.2 In `builder.rs`, replace `handle_error(L2Retry)` + match with a call to `run_recovery_cascade("llm_sample", ...)` in the `Err(e)` arm of `StepSample::execute`
- [x] 4.3 Handle `RecoveryAction::Recovered(msg)` → continue iteration
- [x] 4.4 Handle `RecoveryAction::FailFast(reason)` → yield `SessionEnded(Error)` + return
- [x] 4.5 Yield `AgentEvent::RecoveryApplied { level_number: 3|4|5, tool_name: Some("llm_sample"), message, iteration }` for each cascade action

## 5. Tool Execution Error Path → Cascade

- [x] 5.1 Write failing integration test: tool execution error → cascade L3 fallback injects message as `ToolResult { is_error: true, output: fallback_message }` into `ctx.messages`
- [x] 5.2 In `builder.rs`, replace the `Err(e)` arm of `StepToolExecute::execute` with a call to `run_recovery_cascade(tool_name, ...)`
- [x] 5.3 Inject the `Recovered(message)` as a fallback `ToolResult` with `is_error: true`
- [x] 5.4 Handle `FailFast(reason)` → yield `SessionEnded(Error)` + return
- [x] 5.5 Yield `AgentEvent::RecoveryApplied { level_number: 3|4|5, tool_name: Some(actual_tool_name), message, iteration }` for each cascade action

## 6. AgentRunConfig Compaction Provider

- [x] 6.1 Inspect `crates/synthia-agent/src/config.rs` to confirm `compaction_provider` field presence
- [x] 6.2 If absent, add `pub compaction_provider: Option<Arc<dyn CompactionProvider>>` field
- [x] 6.3 Update `AgentRunConfig::default()` to initialize the new field as `None`
- [x] 6.4 Pass the new field into `run_recovery_cascade` calls in builder.rs

## 7. End-to-End Integration Tests

- [x] 7.1 E2E test: 3 consecutive LLM sampling errors → L5 reset → `ctx.messages` cleared, session can continue on 4th call
- [x] 7.2 E2E test: tool execution error with registered fallback (e.g. `bash`) → L3 fallback → next iteration has fallback message in context
- [x] 7.3 E2E test: oversized tool result (50KB) → L1 truncate → `RecoveryApplied { level_number: 1 }` is emitted

## 8. Verification

- [x] 8.1 Run `cargo +nightly fmt --all` — must be clean
- [x] 8.2 Run `cargo clippy --all-targets --all-features --tests --all` — must be 0 errors, 0 new warnings
- [x] 8.3 Run `cargo test -p synthia-agent` — all tests pass, new tests included
- [x] 8.4 Run `openspec validate explicit-recovery-paths` — must pass
- [x] 8.5 Run `openspec validate` on the 5 archive specs (`auto-compact-on-error`, `session-reset`, `tool-fallback`, `tool-output-truncate`, `tool-retry`) — must continue to pass
- [x] 8.6 Run `cargo test --workspace` to ensure no regression in other crates (allow pre-existing `synthia-session` compile errors per established baseline)
