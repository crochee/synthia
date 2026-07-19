# loop-detector-unified Specification

## Purpose
TBD - created by archiving change loop-detector-unification. Update Purpose after archive.
## Requirements
### Requirement: LoopDetectorSet shall be the single canonical loop detection type in synthia-guardian

The `synthia_guardian::LoopDetectorSet` SHALL be the only public type for loop detection in the Synthia workspace. Other modules (e.g. `synthia-agent`) MUST consume this type via the `synthia_guardian` crate; they MUST NOT define their own `LoopDetectorSet` or equivalent loop detection struct.

#### Scenario: Single source of truth
- **WHEN** A caller (agent, e2e test, 3rd-party integration) needs loop detection
- **THEN** they SHALL use `synthia_guardian::LoopDetectorSet`
- **AND** no other crate in the workspace SHALL define a `LoopDetectorSet` type

#### Scenario: Removal of duplicate agent implementation
- **WHEN** `synthia-agent` is rebuilt after the migration
- **THEN** the file `crates/synthia-agent/src/stream_builder/loop_detection.rs` SHALL NOT exist
- **AND** `synthia-agent/src/dependencies.rs` SHALL import from `synthia_guardian::LoopDetectorSet`

### Requirement: LoopDetectorSet shall combine five independent detectors

The `LoopDetectorSet` SHALL compose exactly five detectors, evaluated in this order: DoomLoop → GenericRepeat → PingPong → PollNoProgress → GlobalCircuit. The first detector that reports a non-`Ok` status SHALL determine the result returned by `check()`.

#### Scenario: Detector order
- **WHEN** `check()` is called with a tool call
- **THEN** DoomLoop SHALL be evaluated first
- **AND** GenericRepeat SHALL be evaluated second
- **AND** PingPong SHALL be evaluated third
- **AND** PollNoProgress SHALL be evaluated fourth (only via `check_poll_result`)
- **AND** GlobalCircuit SHALL be evaluated last (via iteration counter)

#### Scenario: First-detector-wins short-circuit
- **WHEN** DoomLoop returns `Detected`
- **THEN** `check()` SHALL return immediately with `(Detected, Some(RequirePermission))`
- **AND** other detectors SHALL NOT be evaluated in that call

### Requirement: GenericRepeatDetector shall use O(1) HashMap counters

The `GenericRepeatDetector` SHALL store call counts in a `HashMap<(u64, u64), u32>` keyed by `(tool_id, args_hash)`. Each `check()` call SHALL perform an amortized O(1) `HashMap` lookup + update, with NO per-call `String` allocation and NO O(N) scan.

#### Scenario: O(1) repeat detection
- **WHEN** the same `(tool_name, args_json)` is checked N times
- **THEN** each call SHALL complete in amortized O(1) time
- **AND** total memory SHALL grow as O(unique tool/args pairs), NOT O(N)

#### Scenario: Two-tier threshold with warn and block
- **WHEN** count for a `(tool_id, args_hash)` reaches `warn_threshold - 1` (default 2)
- **THEN** `check()` SHALL return `LoopStatus::Warning`
- **WHEN** count reaches `block_threshold` (default 3)
- **THEN** `check()` SHALL return `LoopStatus::Detected`

### Requirement: DoomLoopDetector shall detect three consecutive identical calls

The `DoomLoopDetector` SHALL maintain a sliding window of the last 3 `(tool_name, args_json)` pairs. When the window is full (3 entries) and all 3 are equal, `check()` SHALL return `Detected` and the caller SHALL receive `LoopAction::RequirePermission`.

#### Scenario: Three identical calls trigger
- **WHEN** the same `(tool_name, args_json)` is checked 3 consecutive times
- **THEN** `check()` SHALL return `LoopStatus::Detected` on the 3rd call
- **AND** the 1st and 2nd calls SHALL return `LoopStatus::Ok`

#### Scenario: Interruption resets the window
- **WHEN** call N+1 has different `tool_name` or `args_json` than call N
- **THEN** the oldest entry SHALL be evicted from the window
- **AND** the window count SHALL start over from 0 for the new pattern

### Requirement: PingPongDetector shall detect A-B-A-B alternation

The `PingPongDetector` SHALL maintain a history of recent tool names. When the last 4 calls form an `A-B-A-B` pattern where `A != B`, `check()` SHALL return `Detected`.

#### Scenario: Alternating two tools trigger
- **WHEN** tool calls alternate as `A, B, A, B` (with `A != B`)
- **THEN** `check()` SHALL return `LoopStatus::Detected` on the 4th call

#### Scenario: Three or more unique tools do not trigger
- **WHEN** 4 consecutive calls have 3 or more distinct tool names
- **THEN** `check()` SHALL return `LoopStatus::Ok`

#### Scenario: History is bounded
- **WHEN** history length exceeds 20 entries
- **THEN** the oldest entries SHALL be dropped to keep memory bounded

### Requirement: PollNoProgressDetector shall detect identical poll results

The `PollNoProgressDetector` SHALL hash each `result` string and count consecutive identical hashes. When the count reaches `POLL_NO_PROGRESS_THRESHOLD` (default 10), `check()` SHALL return `Detected`.

#### Scenario: Identical poll results trigger
- **WHEN** the same `result` string is passed to `check_poll_result()` 10 times consecutively
- **THEN** the 10th call SHALL return `LoopStatus::Detected`
- **AND** calls 1-9 SHALL return `LoopStatus::Ok`

### Requirement: GlobalCircuitDetector shall track total iterations

The `GlobalCircuitDetector` SHALL track the current iteration via the `iteration` argument passed to `check()`. When `iteration >= max_iterations` (default 30), `check()` SHALL return `Detected` with `LoopAction::HardBlock`.

#### Scenario: Iteration limit reached
- **WHEN** `check()` is called with `iteration >= 30`
- **THEN** it SHALL return `LoopStatus::Detected` with `LoopAction::HardBlock`
- **WHEN** `iteration < 30`
- **THEN** it SHALL return `LoopStatus::Ok`

### Requirement: LoopDetectorSet API shall be hash-based and consistent

The public API of `LoopDetectorSet` SHALL accept `(tool_name: &str, args_json: &str, iteration: usize)` and return `(LoopStatus, Option<LoopAction>)`. The internal hash computation SHALL be performed once per call via `hash_tool_args()` and shared across detectors.

#### Scenario: Public API signature
- **WHEN** a caller invokes `check()`
- **THEN** the signature SHALL be `pub fn check(&mut self, tool_name: &str, args_json: &str, iteration: usize) -> (LoopStatus, Option<LoopAction>)`
- **AND** the caller SHALL pass `args_json` as the raw JSON string (not pre-hashed)
- **AND** the function SHALL internally call `hash_tool_args()` once

#### Scenario: Detached detector access
- **WHEN** a caller needs to check poll results separately from tool calls
- **THEN** `LoopDetectorSet` SHALL provide `check_poll_result(result: &str) -> LoopStatus`
- **AND** this call SHALL update PollNoProgress state without affecting other detectors

#### Scenario: Reset behavior
- **WHEN** `reset()` is called on `LoopDetectorSet`
- **THEN** all five detectors SHALL clear their internal state
- **AND** the next `check()` call SHALL start from a clean slate

### Requirement: DoomLoop shall surface RequirePermission as caller action

The `LoopAction::RequirePermission` variant SHALL be returned alongside `LoopStatus::Detected` when DoomLoop triggers. This signals to the caller that the LLM has produced 3 identical tool calls in a row and that the caller SHOULD invoke a user-permission check (mirrors opencode's `doom_loop` permission category) before deciding whether to execute the tool.

#### Scenario: DoomLoop returns RequirePermission
- **WHEN** DoomLoopDetector returns `Detected`
- **THEN** `LoopDetectorSet::check()` SHALL return `(LoopStatus::Detected, Some(LoopAction::RequirePermission))`

#### Scenario: GenericRepeat returns Block
- **WHEN** GenericRepeatDetector returns `Detected` (count >= 3)
- **THEN** `LoopDetectorSet::check()` SHALL return `(LoopStatus::Detected, Some(LoopAction::Block))`

#### Scenario: GlobalCircuit returns HardBlock
- **WHEN** GlobalCircuitDetector returns `Detected` (iteration >= 30)
- **THEN** `LoopDetectorSet::check()` SHALL return `(LoopStatus::Detected, Some(LoopAction::HardBlock))`

#### Scenario: GenericRepeat warning returns Warn
- **WHEN** GenericRepeatDetector returns `Warning` (count == 2)
- **THEN** `LoopDetectorSet::check()` SHALL return `(LoopStatus::Warning, Some(LoopAction::Warn))`

#### Scenario: Clean call returns Ok and no action
- **WHEN** no detector reports non-Ok
- **THEN** `LoopDetectorSet::check()` SHALL return `(LoopStatus::Ok, None)`

