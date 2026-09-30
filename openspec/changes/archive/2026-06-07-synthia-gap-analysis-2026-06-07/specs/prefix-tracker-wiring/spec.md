# prefix-tracker-wiring Specification

## ADDED Requirements

### Requirement: PrefixTracker SHALL record system prefix before and after LLM call

`PrefixTracker` SHALL be invoked by `StreamBuilder::run` to record the system prompt byte snapshot immediately before and immediately after each LLM call. The tracker SHALL compute and store a SHA-256 hash of each snapshot.

#### Scenario: Pre-call recording
- **WHEN** `StreamBuilder::run` is about to call `model_call`
- **THEN** it SHALL call `prefix_tracker.record_pre(system_snapshot)` exactly once
- **THEN** `system_snapshot` SHALL return the same bytes `ContextAssembler::system_snapshot()` returned

#### Scenario: Post-call recording
- **WHEN** `StreamBuilder::run` has received the LLM response
- **THEN** it SHALL call `prefix_tracker.record_post(system_snapshot)` exactly once
- **THEN** the post-call snapshot SHALL be byte-identical to the pre-call snapshot (system prompt is immutable in a single LLM call)

#### Scenario: Hash is deterministic
- **WHEN** the same `Vec<u8>` is recorded twice
- **THEN** both recordings SHALL produce the same SHA-256 hash value
- **THEN** hash collisions SHALL be effectively impossible (SHA-256 collision resistance)

### Requirement: PrefixTracker SHALL compute stability ratio

`PrefixTracker` SHALL compute a `stability_ratio` over a rolling window of 20 turns, defined as `(turns with matching pre/post hash) / (total turns in window)`.

#### Scenario: All stable
- **WHEN** 20 consecutive LLM calls all have matching pre/post hashes
- **THEN** `stability_ratio` SHALL return `1.0` (or `100%`)

#### Scenario: Half stable
- **WHEN** 10 out of 20 consecutive LLM calls have matching pre/post hashes
- **THEN** `stability_ratio` SHALL return `0.5` (or `50%`)

#### Scenario: Rolling window
- **WHEN** turn 21 occurs and the window is full (20 entries)
- **THEN** the oldest entry SHALL be evicted
- **THEN** turn 21 SHALL be added
- **THEN** `stability_ratio` SHALL be computed over turns 2-21

#### Scenario: Empty window
- **WHEN** no LLM calls have been recorded
- **THEN** `stability_ratio` SHALL return `1.0` (vacuously stable)
- **THEN** telemetry SHALL NOT report `0.0` as "100% unstable"
- **THEN** the implementation SHALL NOT return `Option<f64>` (no `None` value) — `1.0` is the canonical "no data" value

### Requirement: Stability ratio SHALL be reported to telemetry

`PrefixTracker` SHALL emit a telemetry event `prefix_stability_observed` with the current `stability_ratio` value after each LLM call.

#### Scenario: Event emission
- **WHEN** an LLM call completes and post-call recording happens
- **THEN** `PrefixTracker` SHALL emit `prefix_stability_observed` event
- **THEN** the event SHALL include the current rolling `stability_ratio` as a f64 in [0.0, 1.0]
- **THEN** the event SHALL include the current turn_id (u64)

#### Scenario: Telemetry counter exposed
- **WHEN** telemetry collector subscribes to `prefix_stability_observed`
- **THEN** it SHALL be able to aggregate these into OTel-style metric `codex.prefix.stability` (or equivalent `synthia.context.prefix_stability`)

#### Scenario: Event delivery is non-blocking
- **WHEN** `record_post` is called
- **THEN** event emission SHALL NOT block the LLM call hot path by more than 1ms
- **THEN** event delivery failures SHALL be logged but SHALL NOT fail the LLM call

### Requirement: PrefixTracker SHALL be the only implementation

The workspace SHALL contain exactly one `PrefixTracker` struct, located in `synthia-context/src/prefix_tracker.rs`. Any other crate implementing prefix-hash tracking SHALL be removed or refactored to delegate to this implementation.

#### Scenario: Single struct definition
- **WHEN** searching the workspace for `struct PrefixTracker`
- **THEN** exactly one definition SHALL be found in `synthia-context`
- **THEN** no other crate SHALL define a struct with this name

#### Scenario: telemetry::context_trace delegates
- **WHEN** `synthia-telemetry::context_trace` previously maintained its own prefix hash logic
- **THEN** it SHALL be refactored to call `PrefixTracker::compute_hash` or removed
- **THEN** no duplicate SHA-256 implementations SHALL exist

### Requirement: StreamBuilder SHALL be the only caller of PrefixTracker

`StreamBuilder::run` SHALL be the only place that invokes `PrefixTracker::record_pre` and `record_post`. No other component SHALL record prefix state.

#### Scenario: No other callers
- **WHEN** searching the workspace for `prefix_tracker.record_pre` or `record_post`
- **THEN** only `StreamBuilder::run` (and its tests) SHALL match
- **THEN** no production code outside `synthia-agent::stream_builder` SHALL invoke these methods

#### Scenario: E2E integration
- **WHEN** a full agent session is run end-to-end (LLM → tool → LLM → tool)
- **THEN** `PrefixTracker` SHALL record exactly 2 pre/post pairs (2 LLM calls)
- **THEN** both pairs SHALL have matching pre/post hashes (system prompt unchanged)
- **THEN** `stability_ratio` SHALL be `1.0` at session end

### Requirement: Backward compatibility SHALL be preserved

Existing `PrefixTracker` public API (`compute_prefix_hash`, `record_prefix`, `stability_ratio`) SHALL remain accessible. The wiring SHALL add new call sites without removing the public methods.

#### Scenario: Existing API still callable
- **WHEN** external test code calls `PrefixTracker::compute_prefix_hash(bytes)`
- **THEN** the method SHALL still exist and return the same SHA-256 value as before
- **THEN** compilation SHALL succeed without any external code change

#### Scenario: Existing fields preserved
- **WHEN** `PrefixTracker` struct is modified to support rolling window
- **THEN** existing public fields (if any) SHALL remain with the same names
- **THEN** field types MAY be extended (e.g., `VecDeque<(turn_id, hash)>`) but existing fields SHALL not be renamed
