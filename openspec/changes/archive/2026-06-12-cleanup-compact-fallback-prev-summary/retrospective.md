# Retrospective: cleanup-compact-fallback-prev-summary

> Change: `cleanup-compact-fallback-prev-summary` (archived as `2026-06-12-cleanup-compact-fallback-prev-summary`)
> Scope: FU.2 + FU.5 from [`compact-truncate-prune-convergence`](../../archive/2026-06-12-compact-truncate-prune-convergence/) retrospective

## 1. Outcome

Both FU.2 and FU.5 closed in a single change. Commit `ad4dd99`.

| Item | Result |
|------|--------|
| FU.2 (precomputed_original_tokens) | ✅ Done |
| FU.5 (truncate_previous_summary) | ✅ Done |
| 7 new unit/integration tests | ✅ Pass |
| `cargo +nightly fmt --all` | ✅ 0 new diff |
| `cargo clippy -p synthia-context --all-targets --all-features --tests` | ✅ 0 new warning in `compactor.rs` or `recovery_cascade.rs` |
| `cargo test -p synthia-context` | ✅ 80/80 in `compaction::` |
| `cargo test -p synthia-agent --lib` | ✅ 491/491 |
| `openspec validate` | ✅ green |
| `openspec archive` | ✅ specs synced to baseline |

## 2. What worked

### 2.1 D1 (Option<usize> parameter) held up
- 5 field signatures extended (2 functions + 3 callers). No `CompactOptions` struct needed.
- Backward compatibility trivially preserved via `Option<usize>` + `None` default in tests.
- The pipeline `try_l4_compact → compact_with_fallback → compact_level1` flows the value through with no struct/wrapper.

### 2.2 D3 (4000 char cap) held up
- 4 lines of helper (`truncate_previous_summary`) + 3 small integration points.
- UTF-8 safety pattern (is_char_boundary floor/ceil) reused from the parent change's P0 fix.
- Marker format aligns with `truncate::truncate_output` style.

### 2.3 The 2 pre-existing tests in FU.2 design served as a useful check
- The `test_compact_level1_falls_back_to_estimate_when_none` test caught an off-by-4 in the expected value (the design assumed `100/4 = 25` but `estimate_message_tokens` returns `4 + 100/4 = 29` due to the 4-token base overhead). Updated the test to reflect the actual estimator contract.

## 3. Issues encountered

### 3.1 Dead code in `CapturingProvider`
- The original FU.2 design specified a `last_original_tokens: Mutex<Option<usize>>` field to observe the precomputed value flowing into L1.
- This turned out to be unreachable from `generate_summary` (the field would need to be set by the outer L1 wrapper, not the provider). The actual propagation is observable through `CompactionPart.original_tokens` directly.
- **Decision**: removed both the field and the `record_original_tokens` setter. 5 test initializers simplified in the same edit.
- **Lesson**: when designing test observation points, prefer the public API return value over adding a private field for observation. The wrapper boundary is the right place to extract the value.

### 3.2 Tail slice bug in initial `truncate_previous_summary` draft
- First implementation had a `floor_to_char_boundary_at(prev, tail_start)` helper that returned `&prev[..tail_start]` (slice from start), not `&prev[tail_start..]` (slice to end). Resulted in `dropped = prev.len() - (head.len() + tail.len())` underflow panic.
- **Fix**: removed the helper, inlined two boundary loops (one floors the head, one ceils the tail start). Clearer and one fewer abstraction.
- **Lesson**: "floor to char boundary" is a boundary-index operation, not a slice operation. Naming it `_at` was misleading — the helper was indexing into a conceptual start position, but the API returned a slice from the start.

### 3.3 Spec delta naming
- The delta spec was placed in `specs/compaction-single-pass/` which is a non-standard name (the parent change's archive uses a different name). This is a follow-up consideration for the next retrospective on spec organization.

## 4. Follow-ups (for next gap evaluation)

| ID | Item | Priority | Rationale |
|----|------|----------|-----------|
| FU.4 | Unify `extract_message_text` and `extract_message_tool_uses` with provider's own extractor (if exists) | Low | Pre-existing duplication; only address when modifying either |
| FU.6 | Auto-invoke `prune()` in stream builder | Deferred | Confirmed by adversarial review that production loop never pushes tool results into `ctx.messages`; current pipeline is correct |
| FU.7 | 6 cleared-placeholder tests in `prune-renderer-shape-unification` continue to verify the FU.1 fix | Done | Re-validation only; no new work |
| FU.7.5 | 5 compact-level1-with-provider tests + 5 compact-with-fallback tests + 4 integration tests | Done | Re-validation only; no new work |

## 5. Metrics

| Metric | Before | After | Δ |
|--------|--------|-------|---|
| L4 path `estimate_tokens` calls per trigger | 2 | 1 | -1 (-50%) |
| `previous_summary` growth per L1 | unbounded | capped at 4K + marker | bounded |
| New tests in compactor.rs (this change) | 0 | 7 | +7 |
| Total tests in `compaction::` module | 73 | 80 | +7 |
| Lines changed in `compactor.rs` | — | +395 / -25 | net +370 |
| Lines changed in `recovery_cascade.rs` | — | +4 / 0 | net +4 |

## 6. Next gap evaluation (carried from previous retrospective)

The previous retrospective identified 3 high-value next gaps:
1. **Codex session/Turn model** — high value, but speculative (no concrete production use case)
2. **OpenCode v2 + ACP** — high value, but requires reading new codebase
3. **`synthia-exec` split into `synthia-tool-bash` + `synthia-tool-exec-base`** — medium value, low risk

Recommendation for the user: pick one of {1, 2} for the next P0 evaluation (multi-expert adversarial review), or close 3 as a quick win to stabilize the codebase before tackling the bigger architectural decisions.
