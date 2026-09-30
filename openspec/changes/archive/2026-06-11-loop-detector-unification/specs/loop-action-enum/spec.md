# loop-action-enum Specification

## Purpose

Define the `LoopAction` enum in `synthia_guardian` with five unit variants (`Continue`, `Warn`, `Block`, `RequirePermission`, `HardBlock`) that complement `LoopStatus` and tell the caller how to respond to a loop detection result.

## ADDED Requirements

### Requirement: LoopAction shall enumerate five distinct responses

The `LoopAction` enum SHALL be defined in `synthia_guardian` with exactly five variants: `Continue`, `Warn`, `Block`, `RequirePermission`, `HardBlock`. Each variant SHALL correspond to a specific caller response.

#### Scenario: Enum definition
- **WHEN** `LoopAction` is imported from `synthia_guardian`
- **THEN** it SHALL have the signature: `pub enum LoopAction { Continue, Warn, Block, RequirePermission, HardBlock }`
- **AND** all variants SHALL be unit variants (no payload)
- **AND** the enum SHALL derive `Debug, Clone, Copy, PartialEq, Eq`

#### Scenario: Variant-to-detector mapping
- **WHEN** a caller sees a `LoopAction` value
- **THEN** `Continue` SHALL mean: no loop detected, execute normally
- **AND** `Warn` SHALL mean: GenericRepeat near threshold, log a warning but execute
- **AND** `Block` SHALL mean: standard loop (GenericRepeat, PingPong, PollNoProgress), skip execution
- **AND** `RequirePermission` SHALL mean: DoomLoop, invoke `permission.ask` before execution
- **AND** `HardBlock` SHALL mean: GlobalCircuit, terminate the entire agent loop

---

### Requirement: LoopAction shall be returned as Option from check()

The `LoopDetectorSet::check()` function SHALL return `(LoopStatus, Option<LoopAction>)`. The `Option` SHALL be `Some(action)` when the status is `Warning` or `Detected`, and `None` when the status is `Ok`.

#### Scenario: Ok status has no action
- **WHEN** no detector triggers
- **THEN** `check()` SHALL return `(LoopStatus::Ok, None)`

#### Scenario: Warning status carries Warn action
- **WHEN** GenericRepeat reaches `warn_threshold - 1`
- **THEN** `check()` SHALL return `(LoopStatus::Warning, Some(LoopAction::Warn))`

#### Scenario: Detected status carries an action
- **WHEN** any detector returns `Detected`
- **THEN** `check()` SHALL return `(LoopStatus::Detected, Some(<action>))`
- **AND** `<action>` SHALL be the appropriate variant per the detector mapping

---

### Requirement: LoopAction shall not duplicate information already in LoopStatus

`LoopAction` SHALL be a complementary signal to `LoopStatus`, not a replacement. The pair `(LoopStatus, LoopAction)` SHALL together give the caller both the severity and the recommended response, without redundancy.

#### Scenario: No duplicate fields
- **WHEN** the caller inspects the tuple
- **THEN** `LoopStatus` SHALL indicate the severity (Ok / Warning / Detected)
- **AND** `LoopAction` SHALL indicate the recommended response (Continue / Warn / Block / RequirePermission / HardBlock)
- **AND** these two pieces of information SHALL be derivable independently

---

### Requirement: LoopAction shall be serde-compatible for telemetry

`LoopAction` SHALL derive `Serialize` and `Deserialize` so it can be included in structured logging, metrics labels, and event payloads without custom conversion code.

#### Scenario: Serialize to JSON
- **WHEN** `LoopAction::RequirePermission` is serialized
- **THEN** it SHALL produce the JSON string `"RequirePermission"`
- **AND** all other variants SHALL follow the same pattern (PascalCase variant name)

#### Scenario: Deserialize from JSON
- **WHEN** the JSON string `"HardBlock"` is deserialized
- **THEN** it SHALL produce `LoopAction::HardBlock`
- **AND** unknown variant strings SHALL return a `serde` error
