# Verify: explicit-recovery-paths

> Written: 2026-06-13 (after merge to master)
> Branch: `explicit-recovery-paths` (merged into master @ e4c8d3e)
> Base commit: `c0f8ff1`

---

## 0. Evidence

- **Commits**: 7 (1 event schema + 1 state + 1 L1 + 1 L3-L5 refactor + 1 tool cascade + 1 config + 1 E2E tests)
- **Files changed**: 14 (3 in synthia-agent core, 2 in synthia-agent tests, 1 in synthia-permission, 4 cross-crate updates for `compaction_provider` field)
- **Test delta**: +8 passing tests (L1 truncate × 2, LLM error cascade × 3, tool error cascade × 3)

Commit chain:
```
e4c8d3e (HEAD -> master) merge: explicit-recovery-paths (L1-L5 recovery cascade wired into agent loop)
1dcfad2 test(agent): E2E tests for tool error cascade and L5 reset
f2daeb4 feat(agent): AgentRunConfig carries compaction_provider for L4 cascade
52bbb02 feat(agent): tool execution errors trigger L3-L5 recovery cascade
a92a19c refactor(agent): RecoveryAction::Recovered carries level (3/4/5) tuple
a243776 feat(agent): L1 truncate tool results before context injection
335ee95 feat(agent): BuilderSteps carries reset + failure_tracker state
0143e33 feat(agent): add AgentEvent::RecoveryApplied variant
c0f8ff1 (base) refactor(types): unify TokenUsage across 4 crates via 1-line shims
```

---

## 1. Spec Compliance

| Requirement | Status |
|-------------|--------|
| L1 Tool-Result Truncation | ✅ Implementation + tests |
| L3-L5 Cascade Wired Into LLM Sampling Error Path | ✅ Implementation + tests |
| L3-L5 Cascade Wired Into Tool Execution Error Path | ✅ Implementation + tests |
| RecoveryApplied Event Schema | ✅ Implementation + match in sse.rs |
| BuilderSteps Carries Cascade State | ✅ Implementation + construction |
| Recovery Coordination Does Not Mutate Error Result Semantics | ✅ `RecoveryLevel` no `Serialize` derive, error_recovery/* public API stable |
| Cascade Is Not Invoked for Successful Operations | ✅ Match arms only in error branches |

---

## 2. Verification Results

| Check | Result |
|-------|--------|
| `cargo test -p synthia-agent --test explicit_recovery_paths_test` | 8 passed; 0 failed |
| `cargo test -p synthia-agent` (post-merge) | 524+ passed before first stop on pre-existing `test_multi_turn_memory_with_tracking_provider` |
| `cargo clippy -p synthia-agent --all-targets --all-features --tests` | 0 NEW warnings |
| `openspec validate explicit-recovery-paths` | valid (was valid before merge) |

Pre-existing failures in `synthia-agent` (15) confirmed unchanged from baseline.

Pre-existing `synthia-session/tests/session_persistence.rs` compile error (dual `Session` type confusion) confirmed unchanged — not introduced by this change. Tracked as separate follow-up.

---

## 3. Cross-Crate Compatibility

`AgentRunConfig.compaction_provider` is a new required field. Updated all 5 call sites:
- `crates/synthia-agent/src/agent.rs` (resume_from_checkpoint) — `compaction_provider: None`
- `crates/synthia-server/src/state.rs` — `compaction_provider: None`
- `crates/synthia-server/src/routes/ws.rs` — `compaction_provider: None`
- `crates/synthia-server/src/routes/chat.rs` — `compaction_provider: None`
- `crates/synthia-cli/src/repl_core/repl.rs` — `compaction_provider: None`

`AgentEvent::RecoveryApplied` added to `synthia-server/src/sse.rs` match for `event_variant_name`.

---

## 4. Delta Spec Sync

Delta spec `recovery-cascade-wiring` synced to `openspec/specs/recovery-cascade-wiring/spec.md` at archive time.

---

## 5. Open Items

None blocking. The change is merged into master.
