## Why

Session resume is broken — `resume()` silently drops all checkpoint state, so users cannot resume an interrupted agent session. Additionally, the error recovery cooldown mechanism fires on every error including escalation attempts, blocking legitimate retries and causing sessions to fail fast instead of recovering.

## What Changes

**Agent Resume State**
- From: `run_stream_with_state` destructures `AgentRunStateConfig` and drops `initial_messages` and `start_iteration`
- To: `StreamBuilder::with_initial_state()` propagates restored messages and iteration to the loop
- Reason: `resume()` is a documented feature that must work
- Impact: Non-breaking, restores intended behavior

**ErrorRecovery Cooldown**
- From: `last_recovery_time` stored unconditionally on every `handle_error` call; `record_success()` doesn't clear it
- To: Cooldown timestamp stored only on `RecoveryResult::FailFast`; cleared on `record_success()`
- Reason: First error was starting a 5-second cooldown that blocked subsequent legitimate retries
- Impact: Non-breaking, fixes recovery behavior

## Capabilities

### New Capabilities
- `session-resume`: Session checkpoint state (messages + iteration counter) is preserved and replayed on resume
- `cooldown-on-terminal-failure`: Error recovery cooldown only blocks retries after a terminal (`FailFast`) failure

### Modified Capabilities
- `agent-error-recovery`: Error recovery cooldown semantics corrected

## Impact

- `crates/synthia-agent/src/agent.rs` — `run_stream_with_state` implementation
- `crates/synthia-agent/src/stream_builder/builder.rs` — `StreamBuilder` API addition
- `crates/synthia-agent/src/loop_context.rs` — `LoopContext` initialization
- `crates/synthia-agent/src/error_recovery/mod.rs` — cooldown logic
- Tests: no new tests required (fixes restore intended behavior)