# compaction-single-pass Specification

## Purpose

The compaction subsystem in `synthia-context` must perform token estimation in a single pass (not three) and must support summary anchoring across multiple compaction cycles via the `<previous-summary>` mechanism. This eliminates the O(n²)-equivalent cost of repeated `estimate_tokens` calls in the L1→L2→L3 fallback chain and prevents decision loss across repeated compactions.

## ADDED Requirements

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
