<!--
Synced spec for turn-id-label (cumulative format).
Source: openspec/changes/turn-id-mvp/specs/turn-id-label/spec.md
Synced: 2026-06-13 (after user-initiated thaw ahead of 3-month freeze end)
-->

## Purpose
Define a minimal `TurnId(Uuid)` label type for cross-event turn correlation
in observability. The MVP scope is intentionally narrow: a single
`TurnId(Uuid)` newtype, no `Turn` struct, no `TurnStatus` state machine,
no new `AgentEvent` variants, no persistence.

## Requirements

### Requirement: synthia-agent SHALL define a TurnId type (no Turn struct, no TurnStatus)

`crates/synthia-agent/src/turn.rs` SHALL be created. The file SHALL contain ONLY:
- `pub struct TurnId(pub Uuid)` deriving `Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize`
- An `impl TurnId { pub fn new() -> Self { Self(Uuid::new_v4()) } }`
- An `impl Default for TurnId` returning `Self::new()`

The file SHALL NOT define a `Turn` struct, a `TurnStatus` enum, or any other type. The file SHALL be under 30 lines (verified via `wc -l`).

#### Scenario: TurnId type definition
- **WHEN** `crates/synthia-agent/src/turn.rs` is read
- **THEN** a `pub struct TurnId(pub Uuid)` SHALL be defined
- **AND** the struct SHALL derive `Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize`
- **AND** a `pub fn new() -> Self` method returning `Self(Uuid::new_v4())` SHALL be defined
- **AND** a `Default` impl returning `Self::new()` SHALL be defined

#### Scenario: No Turn struct, no TurnStatus enum
- **WHEN** `crates/synthia-agent/src/turn.rs` is read
- **THEN** no `pub struct Turn { ... }` SHALL be defined
- **AND** no `pub enum TurnStatus { ... }` SHALL be defined
- **AND** no other type besides `TurnId` SHALL be defined in the file

#### Scenario: File size under 30 lines
- **WHEN** `wc -l crates/synthia-agent/src/turn.rs` is run
- **THEN** the line count SHALL be at most 30

### Requirement: LoopContext SHALL expose current_turn_id field

`crates/synthia-agent/src/loop_context.rs` SHALL add a `pub current_turn_id: Option<TurnId>` field to the `LoopContext` struct. The existing `pub iteration: usize` field SHALL be retained (not removed). `LoopContext::new` SHALL initialize `current_turn_id` to `None`.

#### Scenario: LoopContext has current_turn_id field
- **WHEN** `crates/synthia-agent/src/loop_context.rs` is read
- **THEN** `LoopContext` SHALL have a `pub current_turn_id: Option<TurnId>` field
- **AND** the existing `pub iteration: usize` field SHALL be retained
- **AND** `LoopContext::new` SHALL set `current_turn_id: None` initially

#### Scenario: Default initialization
- **WHEN** `LoopContext::new(...)` is called
- **THEN** the returned `LoopContext` SHALL have `current_turn_id: None`
- **AND** the returned `LoopContext` SHALL have `iteration: 0`

#### Scenario: assign_new_turn_id helper
- **WHEN** `LoopContext::assign_new_turn_id()` is called on a mutable `LoopContext`
- **THEN** the function SHALL return a fresh `TurnId`
- **AND** the returned `TurnId` SHALL be stored in `current_turn_id`

### Requirement: StreamBuilder SHALL use current_turn_id instead of formatted string

`crates/synthia-agent/src/stream_builder/builder.rs` SHALL replace the legacy `crate::turn_id::format_turn_id(ctx.iteration)` helper invocation with the typed `ctx.current_turn_id` field. The `AgentContext::new` `turn_id` argument SHALL be populated from `ctx.current_turn_id.map(|t| t.0.to_string())` and SHALL fall back to `format!("turn-{}", ctx.iteration)` when no `TurnId` has been assigned.

#### Scenario: Builder uses current_turn_id
- **WHEN** `crates/synthia-agent/src/stream_builder/builder.rs:357-363` is read
- **THEN** the `AgentContext::new(...)` call SHALL derive its `turn_id` argument from `ctx.current_turn_id`
- **AND** the literal `crate::turn_id::format_turn_id` SHALL NOT appear in this file

#### Scenario: No new AgentEvent variants
- **WHEN** `crates/synthia-agent/src/events.rs` is read
- **THEN** no `TurnStarted`, `TurnCompleted`, `TurnFailed`, or `TurnAborted` variant SHALL be present in `AgentEvent`
- **AND** the existing 39 `AgentEvent` variants SHALL be unchanged

### Requirement: The MVP SHALL NOT introduce persistence, status machines, or new events

The implementation SHALL NOT:
- Add a `turns.jsonl` file or any Turn-specific persistence
- Add a `TurnStatus` state machine or transition table
- Add new `AgentEvent` variants for turn lifecycle
- Add RAII `TurnGuard` patterns
- Modify the `SessionStateMachine` to track turn state

#### Scenario: No turn-level persistence
- **WHEN** `crates/synthia-session/src/store.rs` is read
- **THEN** no `save_turn`, `load_turn`, or `append_turn` method SHALL be added
- **AND** no `turns.jsonl` path SHALL appear in the codebase

#### Scenario: No new AgentEvent variants
- **WHEN** `grep -rn "TurnStarted\|TurnCompleted\|TurnFailed\|TurnAborted" crates/` is run
- **THEN** the only matches SHALL be pre-existing dead match arms in `synthia-cli/src/output.rs` (out of MVP scope)
- **AND** no new `AgentEvent` variant SHALL be introduced

#### Scenario: No Turn struct, no TurnStatus enum
- **WHEN** `grep -rn "pub struct Turn\b\|pub enum TurnStatus" crates/` is run
- **THEN** zero matches SHALL appear
- **AND** the only turn-related type defined SHALL be `TurnId`
