## Context

Two P0 bugs in synthia-agent prevent correct session resumption and error recovery:

**Bug 1 — `resume()` silently drops initial state.**
`Agent::resume()` loads checkpoint messages + iteration, wraps them in `AgentRunStateConfig`, but `run_stream_with_state` destructures the config and drops both `initial_messages` and `start_iteration`. The session starts fresh instead of resuming.

**Bug 2 — ErrorRecovery cooldown blocks legitimate retries.**
`ErrorRecoveryCoordinator::handle_error` unconditionally stores `last_recovery_time` on every call (including `Escalated` results). The first failure starts a 5-second cooldown that prevents subsequent retries from running L3/L4/L5 escalation. Additionally, `record_success()` doesn't clear the cooldown timestamp.

Both bugs are confirmed with specific file/line references. No test coverage exists for resume; cooldown tests lock in the buggy behavior.

## Goals / Non-Goals

**Goals:**
- Restore `resume()` to full functionality — checkpoint/session state preserved and replayed
- Fix cooldown semantics so only terminal failures (`FailFast`) trigger cooldown, not escalation attempts
- Add integration test for resume path

**Non-Goals:**
- No change to checkpoint save format or frequency
- No change to error recovery escalation policy (L1→L5 levels remain the same)
- No new features — pure bug fixes

## Decisions

### D1: resume() fix — StreamBuilder initial state

- **選擇**: Add `with_initial_state(messages, iteration)` method to `StreamBuilder`, store `initial_state: Option<(Vec<Message>, usize)>` in the builder
- **理由**: Non-breaking API addition. Existing callers of `StreamBuilder::run` unaffected. Builder pattern allows chaining.
- **已考慮 alternative**: Modify `run()` signature — rejected: would break all callers. Add state to `LoopContext` constructor — rejected: leaks implementation detail.

### D2: ErrorRecovery cooldown — store only on FailFast

- **選擇**: Move `last_recovery_time.store(now)` inside the `FailFast` branch only; clear on `record_success()`
- **理由**: Cooldown should block rapid retry attempts after terminal failure, not after an escalation that might succeed. Clearing on success ensures cooldown expires naturally.
- **已考慮 alternative**: Separate `enter_cooldown()` method — rejected: more code for same effect. Cooldown only on first error — rejected: doesn't match intended semantics (any terminal failure should cooldown).

## Risks / Trade-offs

[Risk] `record_success()` clearing `last_recovery_time` could allow a retry storm if errors succeed intermittently → Mitigation: `consecutive_errors` counter still provides rate limiting; cooldown is a second layer.

[Risk] `with_initial_state()` adds builder state that could be misused → Mitigation: Only used by `run_stream_with_state`; documented as resume-only.

## Migration Plan

N/A — pure bug fixes, no deployment changes. Changes are backward-compatible. Rollback via `git revert`.

## Open Questions

None — both bugs have clear root causes and fix locations.

## Files to Modify

- `crates/synthia-agent/src/stream_builder/builder.rs`
- `crates/synthia-agent/src/agent.rs`
- `crates/synthia-agent/src/loop_context.rs`
- `crates/synthia-agent/src/error_recovery/mod.rs`