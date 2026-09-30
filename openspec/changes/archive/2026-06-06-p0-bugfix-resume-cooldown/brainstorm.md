# Brainstorming: Track A — P0 Bug Fix (resume + ErrorRecovery cooldown)

## Background

Two P0 bugs identified in synthia-agent:
1. `resume()` silently drops `initial_messages` and `start_iteration`, breaking session resume entirely
2. `ErrorRecoveryCoordinator` cooldown logic stores timestamp on every `handle_error` call, not just on `FailFast`, causing first error to lock out 5s of legitimate retries

## Decision Chain

### Q1: resume()修复后，iteration counter 行为？

**Options:**
- A) 从 `start_iteration` 值继续累加（逻辑正确，checkpoint 恢复点继续）
- B) 从 0 开始（`start_iteration` 只用于 debug/logging）

**Decision: A** — iteration counter 从 `start_iteration` 继续累加。Checkpoint 恢复的 context 应该从原停止点继续。

### Q2: ErrorRecovery cooldown 应该只在 FailFast 时存储？

**Analysis:**
- Bug: `last_recovery_time.store(now)` 无条件执行于每次 `handle_error` 调用
- Bug: `record_success()` 不清除 `last_recovery_time`
- Result: 第一次失败就启动 5s cooldown，阻止后续 legitimate retries

**Decision:** Cooldown 只在 `RecoveryResult::FailFast` 时存储。`record_success()` 必须同时清除 `last_recovery_time`。

## Design Trade-offs

### resume() Fix Approach

| Approach | Pros | Cons |
|----------|------|------|
| Add `initial_state` field to `StreamBuilder` | Non-breaking, chainable API | Minor API addition |
| Modify `StreamBuilder::run` signature | Direct | Breaking change for all callers |

**Chosen: Add `with_initial_state()` method** — returns `Self` for chaining, no signature change to `run()`.

### ErrorRecovery Cooldown Fix

| Approach | Pros | Cons |
|----------|------|------|
| Conditional store in `handle_error` | Minimal change | Logic spread across two methods |
| Separate `enter_cooldown()` method | Clearer separation | More code |

**Chosen: Conditional store only on `FailFast` + clear on `record_success()`** — fixes root cause directly.

## Output

Design doc committed to `docs/superpowers/specs/2026-06-06-track-a-p0-bugfix-design.md`.

## Verification

- `cargo test -p synthia-agent` passes
- Integration test for resume: load checkpoint, resume session, verify history preserved
- Error recovery cooldown test updated to reflect corrected semantics