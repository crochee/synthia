## 1. FU.2: extend `compact_level1` signature with precomputed tokens

- [x] 1.1 Add `precomputed_original_tokens: Option<usize>` as last parameter to `compact_level1(messages, provider, previous_summary, precomputed)` in `crates/synthia-context/src/compaction/compactor.rs`
- [x] 1.2 Implementation: when `Some(n)`, use `n` as `original_tokens`; when `None`, call `estimate_tokens(messages)` (existing behavior)
- [x] 1.3 Empty-messages early return must also accept the precomputed value (line 610-616)

## 2. FU.2: extend `compact_with_fallback` signature with precomputed tokens

- [x] 2.1 Add `precomputed_original_tokens: Option<usize>` as last parameter to `compact_with_fallback(messages, budget, provider, previous_summary, precomputed)` in `compactor.rs`
- [x] 2.2 Forward `precomputed` to the inner `compact_level1` call (line 956)

## 3. FU.2: thread precomputed tokens through callers

- [x] 3.1 `apply_compaction` line 877: pass `Some(original_tokens)` to `compact_level1(msgs_to_compact, p, previous_summary, Some(original_tokens))`
- [x] 3.2 `try_l4_compact` (in `recovery_cascade.rs` line 207): pass `Some(original_tokens)` to `compact_with_fallback`
- [x] 3.3 Update 5 existing `compact_level1_*` test calls to add `None` as last arg
- [x] 3.4 Update 5 existing `compact_with_fallback_*` test calls to add `None` as last arg

## 4. FU.2: new tests

- [x] 4.1 `test_compact_level1_uses_precomputed_tokens_when_supplied` — `Some(42_000)` → `result.original_tokens == 42_000`
- [x] 4.2 `test_compact_with_fallback_propagates_precomputed_tokens` — precomputed value flows from `compact_with_fallback` to inner `compact_level1`
- [x] 4.3 `CapturingProvider.last_original_tokens` field removed (was dead code; FU.2 propagation is now observed through the L1 return value, not via the provider)
- [ ] 4.4 `test_try_l4_compact_avoids_duplicate_estimate` (integration in `recovery_cascade.rs`) — deferred; existing FU.2 unit tests on `compact_level1` and `compact_with_fallback` already exercise the propagation path

## 5. FU.5: add `truncate_previous_summary` helper

- [x] 5.1 Add `const PREVIOUS_SUMMARY_MAX_CHARS: usize = 4000` near top of `compactor.rs`
- [x] 5.2 Add `fn truncate_previous_summary(prev: &str, max_chars: usize) -> String` (module-private)
- [x] 5.3 Algorithm: head 60% + tail 40% + marker line `[... N chars truncated ...]`, with `MARKER_BUDGET = 64` reserved
- [x] 5.4 UTF-8 safety: inline `is_char_boundary` floor (head) / ceil (tail) loops
- [x] 5.5 Pass-through: input ≤ max_chars returns as `prev.to_string()` unchanged

## 6. FU.5: 4 new unit tests for `truncate_previous_summary`

- [x] 6.1 `truncate_previous_summary_below_limit_unchanged` — `truncate("short", 4000) == "short"`
- [x] 6.2 `truncate_previous_summary_above_limit_truncated_with_marker` — `truncate(&"x".repeat(8000), 4000)` → len ≤ 4000 + contains marker
- [x] 6.3 `truncate_previous_summary_preserves_head_and_tail` — head 60% content + tail 40% content both present in output
- [x] 6.4 `truncate_previous_summary_handles_multibyte_utf8` — `"你好世界🌍".repeat(2000)` → no panic, valid UTF-8

## 7. FU.5: integrate truncation at 3 call sites

- [x] 7.1 `Compactor::build_structured_summary` line 264: prepend truncation before the `match previous_summary` block
- [x] 7.2 `build_structured_summary_fallback` line 1059: same integration
- [x] 7.3 `Compactor::level1_summary_with_provider` line 234: truncate BEFORE calling `provider.generate_summary(messages, prev)` so the LLM gets the truncated version

## 8. FU.5: 3 new integration tests

- [x] 8.1 `test_build_structured_summary_truncates_previous_summary` — large `previous_summary` → anchor block content ≤ 4000 + marker appears
- [x] 8.2 `test_build_structured_summary_fallback_truncates_previous_summary` — same for fallback
- [x] 8.3 `test_compact_with_provider_threads_truncated_previous_summary` — use `CapturingProvider` to verify the `previous_summary` arg to `generate_summary` is the truncated version

## 9. Quality gates

- [x] 9.1 `cargo +nightly fmt --all` → 0 new diff (in `compactor.rs` and `recovery_cascade.rs`)
- [x] 9.2 `cargo clippy --all-targets --all-features --tests --all` → 0 new warning in `compactor.rs` or `recovery_cascade.rs`
- [x] 9.3 `cargo test -p synthia-context --all-features` → all green (existing + 7 new tests, 80 total in `compaction::` module)
- [x] 9.4 `cargo test -p synthia-exec` → 0 regression
- [x] 9.5 `cargo test -p synthia-agent --lib` → 0 regression (491 tests pass)
- [x] 9.6 `openspec validate cleanup-compact-fallback-prev-summary` → green

## 10. Spec validation

- [x] 10.1 Verify both delta spec files (`compaction-single-pass` and `previous-summary-anchor`) parse
- [x] 10.2 Verify each MODIFIED/ADDED Requirement has at least one `#### Scenario:` (4 hashtags, not 3)

## 11. Commit and archive

- [ ] 11.1 FU.2 commit: `perf(context): compact_with_fallback accepts precomputed_original_tokens to avoid L4 duplicate estimate`
- [ ] 11.2 FU.5 commit: `feat(context): cap previous_summary at 4000 chars to prevent L1 anchor block bloat`
- [ ] 11.3 `openspec archive cleanup-compact-fallback-prev-summary --yes`
- [ ] 11.4 Verify 2 specs synced to `openspec/specs/` baseline
- [ ] 11.5 Write retrospective.md
- [ ] 11.6 Final `git status` → working tree clean
