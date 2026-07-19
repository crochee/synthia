# stream-builder-v2 Specification

## Purpose

Update the `StreamBuilder` to use the unified `synthia_guardian::LoopDetectorSet` and handle the new `(LoopStatus, Option<LoopAction>)` return type. The local `synthia-agent/src/stream_builder/loop_detection.rs` is removed; doom-loop `RequirePermission` is currently treated as a blocking signal until `synthia-permission` is wired into the stream loop.

## ADDED Requirements

### Requirement: StreamBuilder shall use synthia_guardian::LoopDetectorSet for loop detection

The `StreamBuilder` SHALL use `synthia_guardian::LoopDetectorSet` as its loop detection backend. It MUST NOT import or use any `LoopDetectorSet` defined in `synthia-agent`. The local `synthia-agent/src/stream_builder/loop_detection.rs` module SHALL be removed.

#### Scenario: Loop detection dependency direction
- **WHEN** `StreamBuilder` is constructed
- **THEN** its loop detection dependency SHALL be `synthia_guardian::LoopDetectorSet`
- **AND** it SHALL NOT use `synthia_agent::stream_builder::loop_detection::LoopDetectorSet`

#### Scenario: Removal of local loop detection module
- **WHEN** `cargo build -p synthia-agent` runs after migration
- **THEN** the file `crates/synthia-agent/src/stream_builder/loop_detection.rs` SHALL NOT exist
- **AND** `synthia-agent/src/stream_builder/mod.rs` SHALL NOT export any `loop_detection` submodule

---

### Requirement: StreamBuilder shall handle LoopAction::RequirePermission

The `StepExecutor` SHALL inspect `(LoopStatus, Option<LoopAction>)` from `LoopDetectorSet::check()` and invoke `synthia_permission::Permission::ask()` when receiving `LoopAction::RequirePermission`. The user's decision (allow / deny) SHALL determine whether the tool is executed.

#### Scenario: Doom loop triggers permission ask
- **WHEN** `check()` returns `(LoopStatus::Detected, Some(LoopAction::RequirePermission))`
- **THEN** `StepExecutor` SHALL call `permission.ask(DoomLoop { tool, args })`
- **AND** if the user allows, the tool SHALL be executed
- **AND** if the user denies, the tool SHALL be skipped and the loop SHALL continue

#### Scenario: Standard block skips execution
- **WHEN** `check()` returns `(LoopStatus::Detected, Some(LoopAction::Block))` or `(LoopStatus::Detected, Some(LoopAction::HardBlock))`
- **THEN** `StepExecutor` SHALL NOT execute the tool
- **AND** it SHALL log a warning with the detector name
- **AND** it SHALL continue to the next iteration

#### Scenario: Warning is logged but does not block
- **WHEN** `check()` returns `(LoopStatus::Warning, Some(LoopAction::Warn))`
- **THEN** `StepExecutor` SHALL log a warning
- **AND** it SHALL proceed with tool execution

---

### Requirement: StreamBuilder shall pass iteration count to LoopDetectorSet

The `StepExecutor` SHALL pass the current iteration count from `LoopContext` to `LoopDetectorSet::check()` on every call. This enables the `GlobalCircuitDetector` to use the iteration argument instead of maintaining its own counter.

#### Scenario: Iteration argument propagation
- **WHEN** `StepExecutor` calls `check()` for a tool call
- **THEN** the third argument SHALL be `ctx.iteration`
- **AND** the value SHALL match the iteration count in `LoopContext`
