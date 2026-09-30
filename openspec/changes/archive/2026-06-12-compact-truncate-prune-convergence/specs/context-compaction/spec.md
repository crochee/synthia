<!--
Delta spec for context-compaction capability.
Modifies openspec/specs/context-compaction/spec.md
-->

## ADDED Requirements

### Requirement: L1 compaction SHALL preserve prior summary via previous_summary anchor

When `apply_compaction` invokes L1 (LLM summary) and a prior summary from a previous compaction cycle is available, the function MUST pass the prior summary to `compact_level1` via the `previous_summary: Option<&str>` parameter. The L1 prompt template MUST include a `<previous-summary>` block that instructs the LLM to update the prior summary with new facts rather than create a fresh summary from scratch.

#### Scenario: L1 with prior summary includes anchor block
- **WHEN** `apply_compaction` is called and a prior summary string is available from the session state
- **THEN** `compact_level1` SHALL be invoked with `previous_summary = Some(&prior_summary)`
- **AND** the rendered LLM prompt SHALL contain a `<previous-summary>` block enclosing the prior summary text
- **AND** the prompt instruction SHALL be the "Update the anchored summary below..." variant

#### Scenario: L1 with no prior summary uses fresh prompt
- **WHEN** `apply_compaction` is called and no prior summary is available
- **THEN** `compact_level1` SHALL be invoked with `previous_summary = None`
- **AND** the rendered LLM prompt SHALL NOT contain a `<previous-summary>` block
- **AND** the prompt instruction SHALL be the "Create a new anchored summary..." variant

#### Scenario: Successful L1 produces a new summary for the next cycle
- **WHEN** L1 succeeds and returns a new summary string
- **THEN** the caller SHALL store that string in the session state
- **AND** a subsequent `apply_compaction` call within the same session SHALL pass that string as `previous_summary`

### Requirement: L2 and L3 fallback paths SHALL NOT use previous_summary

The L2 (structured truncation) and L3 (marker-only) fallback paths MUST NOT accept a `previous_summary` parameter and MUST NOT include a `<previous-summary>` block in their prompts. This is because L2 and L3 do not generate a new anchored summary; they only truncate or mark.

#### Scenario: L2 has no previous_summary parameter
- **WHEN** the L2 function signature is examined
- **THEN** it SHALL NOT have a `previous_summary` parameter

#### Scenario: L3 has no previous_summary parameter
- **WHEN** the L3 function signature is examined
- **THEN** it SHALL NOT have a `previous_summary` parameter

### Requirement: apply_compaction SHALL perform single-pass token estimation

The `apply_compaction` function MUST call `estimate_tokens` exactly once on the input range. It MUST NOT re-estimate after L1 fails before falling through to L2, and MUST NOT re-estimate after L2 fails before falling through to L3.

#### Scenario: All levels share a single original_tokens value
- **WHEN** `apply_compaction` is called regardless of which level is applied
- **THEN** `estimate_tokens` on the input range SHALL be called exactly once
- **AND** the `CompactionResult.original_tokens` field SHALL be that single value

#### Scenario: L4 cascade path does not duplicate estimation
- **WHEN** the recovery cascade triggers L4 compaction via `try_l4_compact`
- **THEN** the total number of `estimate_tokens` calls across the L4 path SHALL be ≤ 1
- **AND** the recovery cascade SHALL be able to read `original_tokens` from the returned `CompactionResult`
