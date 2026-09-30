# loop-detector-algorithm Specification

## Purpose

Replace the O(N) + N² `GenericRepeatDetector` algorithm (which clones the full `input_json` string and rebuilds a HashMap on every check) with an O(1) HashMap-counter algorithm. Also unify 3 separate `LoopDetector` implementations (guardian, agent, stream_builder) into a single `LoopDetectorSet` in `synthia-guardian`. Replace `try_write` (which silently drops records) with `Mutex` (per concurrency review R5).

## ADDED Requirements

### Requirement: GenericRepeatDetector shall use O(1) HashMap counters

`GenericRepeatDetector` SHALL maintain `HashMap<(u64 /* tool_id */, u64 /* args_hash */), u32>`, replacing the old `VecDeque<(String, u64)>` history.

The detector SHALL NOT clone `input_json` to a `String`. The detector SHALL use `u64` tool_id (e.g., a hash of tool_name) and `u64` args_hash as keys.

`check(tool_id: u64, args_hash: u64)` SHALL:
1. Increment the counter for `(tool_id, args_hash)` in the HashMap
2. Return `LoopStatus::Detected` if the count reaches `max_threshold`
3. Return `LoopStatus::Ok` otherwise

`record_outcome(tool_id: u64, args_hash: u64, success: bool)` SHALL:
- If `success == true`: decrement the counter (saturating at 0); remove entry if counter reaches 0
- If `success == false`: no-op (counter stays)

#### Scenario: First call returns Ok
- **WHEN** `check(0xABCD, 0x1234)` is called for the first time
- **THEN** the internal counter for `(0xABCD, 0x1234)` SHALL be 1
- **THEN** the function SHALL return `LoopStatus::Ok`

#### Scenario: Repeated calls trigger detection at threshold
- **WHEN** `check(0xABCD, 0x1234)` is called `max_threshold` times consecutively
- **THEN** the function SHALL return `LoopStatus::Detected` on the `max_threshold`-th call

#### Scenario: Success decays the counter
- **WHEN** `check(0xABCD, 0x1234)` is called 3 times (counter = 3)
- **AND** `record_outcome(0xABCD, 0x1234, true)` is called once
- **THEN** the counter SHALL be 2
- **THEN** the next `check(0xABCD, 0x1234)` SHALL return `LoopStatus::Ok`

#### Scenario: Zero String allocation
- **WHEN** `check(tool_id, args_hash)` is called
- **THEN** the implementation SHALL NOT call `.to_string()` on any input
- **THEN** no `String` allocation SHALL occur in the hot path

---

### Requirement: LoopDetectorSet shall be the single canonical implementation

`synthia_guardian::loop_detector::LoopDetectorSet` SHALL be the only `LoopDetectorSet` implementation in the workspace.

`synthia_agent::agent::loop_detector::LoopDetector` SHALL be deleted.
`synthia_agent::agent::LoopDetector` references in `core.rs:77`, `react.rs:557-706`, and `step.rs:489` SHALL be replaced with `LoopDetectorSet`.

`synthia-agent/Cargo.toml` SHALL depend on `synthia-guardian` directly (or import via re-export).

#### Scenario: Single source of truth
- **WHEN** `grep -r "pub struct LoopDetector" crates/` is run
- **THEN** the search SHALL return exactly 1 result: `synthia-guardian::loop_detector::LoopDetectorSet`
- **THEN** the search SHALL NOT find `synthia-agent::agent::loop_detector::LoopDetector`

#### Scenario: agent::LoopDetector removed
- **WHEN** the refactor is complete
- **THEN** `crates/synthia-agent/src/agent/loop_detector.rs` SHALL NOT exist
- **THEN** all 6 call sites in `react.rs` (lines 557, 569, 583, 602, 627, 706) SHALL use `LoopDetectorSet` methods

---

### Requirement: LoopDetectorSet shall be wrapped in Mutex, not RwLock

`Agent::loop_detector` SHALL be `Arc<Mutex<LoopDetectorSet>>`, replacing the current `Arc<RwLock<LoopDetector>>`.

`step.rs:489` SHALL use `loop_detector.lock().expect("loop_detector mutex poisoned")` instead of `try_write`.

#### Scenario: try_write is replaced
- **WHEN** the refactor is complete
- **THEN** `grep -r "try_write" crates/synthia-agent/` SHALL return 0 results for `loop_detector`
- **THEN** the call site SHALL block on `lock()` rather than silently dropping the record

#### Scenario: Mutex is the new wrapper
- **WHEN** `Agent::new` constructs the loop detector
- **THEN** the type SHALL be `Arc<Mutex<LoopDetectorSet>>`
- **THEN** `use std::sync::RwLock` SHALL NOT be imported in `agent/core.rs` for the loop_detector field

#### Scenario: No silent record drops
- **WHEN** a tool execution completes and the main loop calls `record(pattern)`
- **THEN** the call SHALL complete (no `if let Ok(...)` fallback)
- **THEN** if the Mutex is poisoned, the program SHALL panic (fail-fast) rather than silently lose the record

---

### Requirement: NoProgressDetector shall use O(1) lookup, not N×M scan

`NoProgressDetector` SHALL maintain incremental state that allows O(1) check, replacing the current O(N×M) scan.

The implementation SHALL track "consecutive_no_new" as a `u32` counter, incrementing when no new tool appears in the window, resetting when a new tool appears.

#### Scenario: O(1) check
- **WHEN** `NoProgressDetector::check()` is called
- **THEN** the implementation SHALL NOT iterate over a window of past calls
- **THEN** the implementation SHALL NOT iterate over a set of unique tools per check
- **THEN** the check SHALL complete in O(1) time

#### Scenario: Behavior matches old semantics
- **WHEN** 5 consecutive tool calls use the same set of tools
- **THEN** `consecutive_no_new` SHALL reach the threshold
- **THEN** `check()` SHALL return `LoopStatus::Detected` (matching old `N×M` scan behavior)
