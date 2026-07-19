# compaction-single-pass Specification (delta)

## ADDED Requirements

### Requirement: compact_level1 MUST accept precomputed_original_tokens parameter

The `compact_level1` function signature MUST be extended to accept `precomputed_original_tokens: Option<usize>` as its last parameter. When `Some(n)` is provided, the function MUST use `n` as the `original_tokens` value in the returned `CompactionPart` and MUST NOT call `estimate_tokens(messages)` internally. When `None` is provided, the existing behavior (call `estimate_tokens(messages)`) MUST be preserved.

The parameter MUST be passed through by:
- `compact_with_fallback` — when the caller supplies `precomputed_original_tokens`, forward it to the inner `compact_level1` call
- `apply_compaction` — MUST pass `Some(original_tokens)` (the value computed at the top of the function) to `compact_level1`
- `try_l4_compact` in `recovery_cascade` — MUST pass `Some(original_tokens)` (the value computed at the top of the function) to `compact_with_fallback`

#### Scenario: precomputed_original_tokens is Some — estimate is skipped
- **WHEN** `compact_level1` is called with `precomputed_original_tokens = Some(42_000)`
- **THEN** the returned `CompactionPart.original_tokens` SHALL equal `42_000`
- **AND** `estimate_tokens(messages)` SHALL NOT be called internally for that call

#### Scenario: precomputed_original_tokens is None — estimate is called
- **WHEN** `compact_level1` is called with `precomputed_original_tokens = None`
- **THEN** the returned `CompactionPart.original_tokens` SHALL equal `estimate_tokens(messages)` (existing behavior)
- **AND** this MUST be backward-compatible with all existing test calls that pass no `precomputed_original_tokens` argument

#### Scenario: compact_with_fallback propagates precomputed value to inner L1
- **WHEN** `compact_with_fallback` is called with `precomputed_original_tokens = Some(9999)` and a provider that succeeds at L1
- **THEN** the inner `compact_level1` call SHALL receive `Some(9999)`
- **AND** the L1's returned `CompactionPart.original_tokens` SHALL equal `9999` (not the internal re-estimate)

#### Scenario: try_l4_compact passes its precomputed value to compact_with_fallback
- **WHEN** `try_l4_compact` triggers the L4 path with `ctx.messages` totaling N tokens
- **THEN** `compact_with_fallback` SHALL receive `precomputed_original_tokens = Some(N)`
- **AND** the inner L1 SHALL return `CompactionPart.original_tokens == N`
- **AND** the total `estimate_tokens` calls across the L4 path SHALL remain ≤ 1 (the one at `try_l4_compact`'s top)

### Requirement: previous_summary MUST be capped at 4000 characters before anchor injection

The compaction subsystem MUST cap any `previous_summary` value at `PREVIOUS_SUMMARY_MAX_CHARS = 4000` characters before injecting it into:
- The structured summary's `<previous-summary>` block in `Compactor::build_structured_summary`
- The structured summary's `<previous-summary>` block in `build_structured_summary_fallback`
- The `previous_summary` argument to `CompactionProvider::generate_summary`

The cap MUST preserve the head (60% of budget, most recent decisions) and the tail (40% of budget, oldest decisions) of the original string, with a `[... N chars truncated ...]` marker line between them. The truncation MUST be UTF-8 safe (slice boundaries floored to the nearest `is_char_boundary` to prevent panics, mirroring the P0 bash UTF-8 fix).

If `previous_summary.len() <= PREVIOUS_SUMMARY_MAX_CHARS`, the value MUST be passed through unchanged.

#### Scenario: previous_summary is below the cap — passed through unchanged
- **WHEN** `previous_summary` is `"Recent decisions: A, B, C"` (length < 4000)
- **THEN** the value passed to the provider and embedded in the anchor block SHALL be the original string unchanged
- **AND** the output MUST NOT contain a `[... N chars truncated ...]` marker

#### Scenario: previous_summary is above the cap — truncated with head/tail/marker
- **WHEN** `previous_summary` is `"x".repeat(8000)` and `PREVIOUS_SUMMARY_MAX_CHARS = 4000`
- **THEN** the truncated string SHALL have total length ≤ 4000 + marker overhead
- **AND** SHALL contain a marker line of the form `[... 4000 chars truncated ...]` (or similar N indicating dropped characters)
- **AND** the head of the truncated output SHALL contain the first ~60% of the original (most recent prefix)
- **AND** the tail of the truncated output SHALL contain the last ~40% of the original (oldest suffix)

#### Scenario: previous_summary with multi-byte UTF-8 above the cap — no panic
- **WHEN** `previous_summary` is `"你好世界🌍".repeat(2000)` (length well above 4000, contains 3-byte and 4-byte UTF-8 sequences)
- **THEN** the truncation MUST NOT panic
- **AND** the output SHALL be valid UTF-8
- **AND** all slice boundaries SHALL be valid `is_char_boundary` points

#### Scenario: previous_summary cap is applied before LLM provider call
- **WHEN** `compact_level1` is called with `previous_summary = Some(long_string)` where `long_string.len() > 4000`
- **THEN** the `previous_summary` argument that `CompactionProvider::generate_summary` receives SHALL be the truncated version (≤ 4000 chars)
- **AND** the original long string MUST NOT be forwarded to the LLM verbatim

#### Scenario: previous_summary cap is applied in structured fallback
- **WHEN** the LLM provider returns empty or errors, and `build_structured_summary_fallback` is invoked with `previous_summary = Some(long_string)`
- **THEN** the `<previous-summary>` block embedded in the fallback output SHALL contain the truncated version (≤ 4000 chars + marker)
- **AND** the rendered summary MUST NOT exceed the budget by the unbounded growth of the anchor block
