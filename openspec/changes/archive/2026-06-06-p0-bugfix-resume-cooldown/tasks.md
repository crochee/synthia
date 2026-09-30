## 1. resume() Fix — StreamBuilder initial state

- [x] 1.1 Add `initial_state: Option<(Vec<Message>, usize)>` field to `StreamBuilder` struct in `builder.rs`
- [x] 1.2 Add `with_initial_state(&mut self, messages: Vec<Message>, iteration: usize) -> &mut Self` method to `StreamBuilder`
- [x] 1.3 Modify `LoopContext::new()` to use `initial_state` if provided (set `ctx.messages` and `ctx.iteration`)
- [x] 1.4 Change `run_stream_with_state` in `agent.rs` to call `with_initial_state(initial_messages, start_iteration)` before `.run()`
- [x] 1.5 Remove the destructuring that drops `initial_messages` and `start_iteration`

## 2. ErrorRecovery Cooldown Fix

- [x] 2.1 In `error_recovery/mod.rs`, move `last_recovery_time.store(now)` inside the `FailFast` match arm only — not on `Escalated`
- [x] 2.2 In `record_success()`, add `self.last_recovery_time.store(0, Ordering::Relaxed)` to clear the cooldown
- [x] 2.3 Update `test_coordinator_cooldown` test to verify: first `Escalated` does not enter cooldown; second `Escalated` within window still returns `Escalated`; only `FailFast` enters cooldown

## 3. Testing

- [ ] 3.1 Add integration test: checkpoint save → kill session → resume → verify all messages preserved and iteration counter correct
- [ ] 3.2 Run `cargo test -p synthia-agent` to confirm all tests pass
- [ ] 3.3 Run `cargo clippy` to confirm clean