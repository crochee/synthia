# prefix-tracker-wiring Specification (delta)

## Purpose

This is a delta spec for the existing `prefix-tracker-wiring` capability. The full specification is at `openspec/specs/prefix-tracker-wiring/spec.md`. This file documents the requirements being added or modified by the `streaming-2part-truncate` change.

## MODIFIED Requirements

### Requirement: PrefixTracker SHALL record system prefix before and after LLM call

`PrefixTracker` SHALL be invoked by `StreamBuilder::run` to record the system prompt byte snapshot immediately before and immediately after each LLM call. The tracker SHALL compute and store a SHA-256 hash of each snapshot. When a `TwoPartPrompt` is used, `PrefixTracker` SHALL additionally record the `TwoPartPrompt::header_hash` (blake3) for cross-validation with the provider's cache hit indicator.

#### Scenario: Pre-call recording
- **WHEN** `StreamBuilder::run` is about to call `model_call`
- **THEN** it SHALL call `prefix_tracker.record_pre(system_snapshot)` exactly once
- **THEN** `system_snapshot` SHALL return the same bytes `ContextAssembler::system_snapshot()` returned
- **THEN** if a `TwoPartPrompt` is in use, it SHALL additionally call `prefix_tracker.record_two_part_header_hash(header_hash)`

#### Scenario: Post-call recording
- **WHEN** `StreamBuilder::run` has received the LLM response
- **THEN** it SHALL call `prefix_tracker.record_post(system_snapshot)` exactly once
- **THEN** the post-call snapshot SHALL be byte-identical to the pre-call snapshot (system prompt is immutable in a single LLM call)

#### Scenario: Hash is deterministic
- **WHEN** the same `Vec<u8>` is recorded twice
- **THEN** both recordings SHALL produce the same SHA-256 hash value
- **THEN** hash collisions SHALL be effectively impossible (SHA-256 collision resistance)

## ADDED Requirements

### Requirement: TwoPartPrompt header_hash SHALL be cross-validated with provider cache hits

`PrefixTracker` SHALL record the `TwoPartPrompt::header_hash` (blake3, 32 bytes) alongside the SHA-256 system snapshot. After the LLM call returns, `PrefixTracker` SHALL compare the recorded header_hash to the upstream provider's `x-cache: hit|miss` header (or equivalent telemetry signal). A divergence between `header_hash` stable and `x-cache: miss` SHALL be reported as a `cache_signal_mismatch` event.

#### Scenario: Both stable and cache hit
- **WHEN** `header_hash` matches the previous turn's value AND the upstream response reports cache hit
- **THEN** `PrefixTracker` SHALL record both as stable; no mismatch event
- **THEN** `prefix_stability_ratio` SHALL increment its stable count

#### Scenario: Stable but cache miss (anomaly)
- **WHEN** `header_hash` matches the previous turn's value BUT the upstream response reports cache miss
- **THEN** `PrefixTracker` SHALL emit a `cache_signal_mismatch` telemetry event
- **THEN** the event SHALL include `header_hash` and the upstream `x-cache` value
- **THEN** this SHALL NOT increment the unstable count (the header is byte-stable; the provider may be doing its own cache eviction)

#### Scenario: Header changed
- **WHEN** `header_hash` differs from the previous turn's value
- **THEN** `PrefixTracker` SHALL record this turn as unstable
- **THEN** `prefix_stability_ratio` SHALL decrement its stable count
- **THEN** no `cache_signal_mismatch` SHALL be emitted (the change explains the miss)

### Requirement: TwoPartPrompt header_hash SHALL use the same prefix_stability_observed event

`PrefixTracker` SHALL continue to emit the `prefix_stability_observed` event after each LLM call. The event SHALL include both the legacy `stability_ratio` (SHA-256 based) and the new `header_hash_stability_ratio` (blake3 based) when `TwoPartPrompt` is in use.

#### Scenario: Event emission with both ratios
- **WHEN** an LLM call completes and `TwoPartPrompt` was used
- **THEN** `prefix_stability_observed` SHALL include `stability_ratio: f64` (legacy)
- **THEN** it SHALL include `header_hash_stability_ratio: f64` (new, over the same rolling window)
- **THEN** it SHALL include the current `turn_id: u64`

#### Scenario: Event emission without TwoPartPrompt
- **WHEN** an LLM call completes and `TwoPartPrompt` was NOT used
- **THEN** `prefix_stability_observed` SHALL include `stability_ratio` only
- **THEN** `header_hash_stability_ratio` SHALL be `None` or absent
