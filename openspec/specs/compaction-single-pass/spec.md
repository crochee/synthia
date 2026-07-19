# compaction-single-pass Specification

## Purpose
TBD - created by archiving change compact-truncate-prune-convergence. Update Purpose after archive.
## Requirements
### Requirement: apply_compaction MUST estimate tokens exactly once per call

The `synthia_context::compaction::compactor::apply_compaction` function MUST call `estimate_tokens` exactly once on the input range, regardless of which compaction level (L1, L2, or L3) is ultimately applied. The function MUST NOT re-estimate after L1 fails before falling through to L2, and MUST NOT re-estimate after L2 fails before falling through to L3.

#### Scenario: L1 succeeds — single estimate
- **WHEN** `apply_compaction` is called and L1 (LLM summary) succeeds within the token budget
- **THEN** `estimate_tokens` SHALL be called exactly once on the input range
- **AND** the returned `CompactionResult.original_tokens` SHALL equal that single estimate
- **AND** `CompactionResult.applied_level` SHALL be `1`

#### Scenario: L1 fails, L2 succeeds — single estimate
- **WHEN** L1 fails and L2 (structured truncation) is applied
- **THEN** `estimate_tokens` SHALL be called exactly once on the input range (the original estimate, not the L2 result estimate)
- **AND** the returned `CompactionResult.original_tokens` SHALL equal the original range estimate
- **AND** `CompactionResult.compacted_tokens` SHALL equal the L2 result's separate estimate (allowed because that is a different message set)
- **AND** `CompactionResult.applied_level` SHALL be `2`

#### Scenario: L1 and L2 both fail, L3 applied — single estimate on input
- **WHEN** L1 and L2 both fail and L3 (marker-only) is applied
- **THEN** `estimate_tokens` on the input range SHALL be called exactly once
- **AND** the returned `CompactionResult.original_tokens` SHALL equal that single estimate
- **AND** `CompactionResult.applied_level` SHALL be `3`

### Requirement: compact_level1 MUST accept previous_summary parameter

The `compact_level1` function signature MUST be extended to accept `previous_summary: Option<&str>`. When `Some(previous_summary)` is provided, the prompt template MUST include a `<previous-summary>` block instructing the LLM to update the prior summary rather than create one from scratch. When `None` is provided, the existing behavior (create new summary) MUST be preserved.

#### Scenario: previous_summary is None — new summary prompt
- **WHEN** `compact_level1` is called with `previous_summary = None`
- **THEN** the rendered prompt MUST contain the instruction "Create a new anchored summary from the conversation history above."
- **AND** MUST NOT contain a `<previous-summary>` block

#### Scenario: previous_summary is Some — anchored prompt
- **WHEN** `compact_level1` is called with `previous_summary = Some("prior decisions: X, Y, Z")`
- **THEN** the rendered prompt MUST contain the block:
  ```
  <previous-summary>
  prior decisions: X, Y, Z
  </previous-summary>
  ```
- **AND** the instruction MUST be "Update the anchored summary below using the conversation history above. Preserve still-true details, remove stale details, and merge in the new facts."

#### Scenario: L2 and L3 paths MUST NOT accept previous_summary
- **WHEN** `compact_level2` or `compact_level3` is invoked
- **THEN** the function signatures MUST NOT have a `previous_summary` parameter
- **AND** the rendered prompts MUST NOT contain a `<previous-summary>` block

### Requirement: apply_compaction MUST pass successful L1 summary as previous_summary to next call

When `apply_compaction` is called and L1 succeeds, the caller MUST be able to retrieve the resulting summary text for use in subsequent `apply_compaction` invocations. The `CompactionResult` MUST include a `summary: SummaryMessage` field whose `summary` string can be passed as the next call's `previous_summary` argument.

#### Scenario: L1 result is retrievable for next call
- **WHEN** `apply_compaction` returns a result with `applied_level = 1`
- **THEN** the result SHALL include `summary.summary: String`
- **AND** that string can be passed to a subsequent `apply_compaction` call as the `previous_summary` argument
- **AND** the subsequent call's prompt MUST include the prior summary in a `<previous-summary>` block

### Requirement: recovery_cascade::try_l4_compact MUST NOT re-estimate

The `synthia_agent::error_recovery::recovery_cascade::try_l4_compact` function MUST share a single `original_tokens` calculation with `apply_compaction`. The function MUST NOT independently re-estimate the input range token count before calling `apply_compaction` and MUST NOT re-estimate after.

#### Scenario: L4 trigger calls apply_compaction without duplicate estimate
- **WHEN** L4 compaction is triggered by the recovery cascade
- **THEN** `try_l4_compact` SHALL call `apply_compaction` (or the underlying `compact_with_fallback`) with the original messages
- **AND** the total number of `estimate_tokens` calls across the L4 path SHALL be ≤ 1 per call
- **AND** the `CompactionResult.original_tokens` field SHALL be propagated back to the caller for any subsequent decision logic

#### Scenario: L4 success is verifiable
- **WHEN** L4 compaction succeeds
- **THEN** the returned result MUST include a `compacted_tokens` value
- **AND** the recovery cascade MUST be able to compare `compacted_tokens < original_tokens` to determine success
- **AND** on success, the consecutive failure counter for the failed tool MUST be reset

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

