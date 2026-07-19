<!--
Delta spec for turn-id-mvp change.
Capability: turn-id-label (new, FROZEN for 3 months)
-->

## ADDED Requirements

### Requirement: TurnId SHALL exist as a frozen MVP spec

A new capability `turn-id-label` SHALL be defined in this change. The capability describes a minimal `TurnId(Uuid)` label type that may be implemented in the future. **The change is FROZEN from 2026-06-13 to 2026-09-13 and SHALL NOT be applied during this period.**

#### Scenario: Change is marked FROZEN
- **WHEN** the `openspec/changes/turn-id-mvp/` directory is inspected
- **THEN** the `proposal.md` SHALL contain a "FROZEN" marker in the Why section
- **THEN** the `tasks.md` SHALL NOT have any active implementation tasks
- **THEN** `openspec status --change "turn-id-mvp"` SHALL show the change in frozen state

#### Scenario: No code changes during frozen period
- **WHEN** the frozen period (2026-06-13 to 2026-09-13) is in effect
- **THEN** no file in `crates/` SHALL be modified to introduce a `TurnId` type
- **THEN** no file in `crates/` SHALL be modified to add a `current_turn_id` field to `LoopContext`

### Requirement: Upon thaw, synthia-agent SHALL define a TurnId type (no Turn struct, no TurnStatus)

When the change is unfrozen, `crates/synthia-agent/src/turn.rs` SHALL be created. The file SHALL contain ONLY:
- `pub struct TurnId(pub Uuid)` deriving `Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize`
- An `impl TurnId { pub fn new() -> Self { Self(Uuid::new_v4()) } }`

The file SHALL NOT define a `Turn` struct, a `TurnStatus` enum, or any other type. The file SHALL be under 30 lines.

#### Scenario: TurnId type definition
- **WHEN** `crates/synthia-agent/src/turn.rs` is read
- **THEN** a `pub struct TurnId(pub Uuid)` SHALL be defined
- **THEN** the struct SHALL derive `Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize`
- **THEN** a `pub fn new() -> Self` method returning `Self(Uuid::new_v4())` SHALL be defined

#### Scenario: No Turn struct, no TurnStatus enum
- **WHEN** `crates/synthia-agent/src/turn.rs` is read
- **THEN** no `pub struct Turn { ... }` SHALL be defined
- **THEN** no `pub enum TurnStatus { ... }` SHALL be defined
- **THEN** no other type besides `TurnId` SHALL be defined in the file

#### Scenario: File size under 30 lines
- **WHEN** `wc -l crates/synthia-agent/src/turn.rs` is run
- **THEN** the line count SHALL be at most 30

### Requirement: Upon thaw, LoopContext SHALL expose current_turn_id field

When the change is unfrozen, `crates/synthia-agent/src/loop_context.rs` SHALL add a `pub current_turn_id: Option<TurnId>` field to the `LoopContext` struct. The existing `pub iteration: usize` field SHALL be retained (not removed). `LoopContext::new` SHALL initialize `current_turn_id` to `None`.

#### Scenario: LoopContext has current_turn_id field
- **WHEN** `crates/synthia-agent/src/loop_context.rs` is read
- **THEN** `LoopContext` SHALL have a `pub current_turn_id: Option<TurnId>` field
- **THEN** the existing `pub iteration: usize` field SHALL be retained
- **THEN** `LoopContext::new` SHALL set `current_turn_id: None` initially

#### Scenario: Default initialization
- **WHEN** `LoopContext::new(...)` is called
- **THEN** the returned `LoopContext` SHALL have `current_turn_id: None`
- **THEN** the returned `LoopContext` SHALL have `iteration: 0`

### Requirement: Upon thaw, StreamBuilder SHALL use current_turn_id instead of formatted string

When the change is unfrozen, `crates/synthia-agent/src/stream_builder/builder.rs` SHALL replace the literal `format!("turn-{}", ctx.iteration)` (currently at line 327) with `ctx.current_turn_id`. The agent context `turn_id` field SHALL be populated from the typed `TurnId` value rather than a runtime-formatted string.

#### Scenario: Hook context uses TurnId type
- **WHEN** `crates/synthia-agent/src/stream_builder/builder.rs:325-328` is read
- **THEN** the `AgentContext::new(...)` call SHALL receive `ctx.current_turn_id` (typed `Option<TurnId>`) as the `turn_id` argument
- **THEN** the literal `format!("turn-{}", ctx.iteration)` SHALL NOT appear in this file

#### Scenario: No new AgentEvent variants
- **WHEN** `crates/synthia-agent/src/events.rs` is read
- **THEN** no `TurnStarted`, `TurnCompleted`, `TurnFailed`, or `TurnAborted` variant SHALL be present in `AgentEvent`
- **THEN** the existing 39 `AgentEvent` variants SHALL be unchanged

### Requirement: The MVP SHALL NOT introduce persistence, status machines, or new events

When the change is unfrozen, the implementation SHALL NOT:
- Add a `turns.jsonl` file or any Turn-specific persistence
- Add a `TurnStatus` state machine or transition table
- Add new `AgentEvent` variants for turn lifecycle
- Add RAII `TurnGuard` patterns
- Modify the `SessionStateMachine` to track turn state

#### Scenario: No turn-level persistence
- **WHEN** `crates/synthia-session/src/store.rs` is read after thaw implementation
- **THEN** no `save_turn`, `load_turn`, or `append_turn` method SHALL be added
- **THEN** no `turns.jsonl` path SHALL appear in the codebase

#### Scenario: No new AgentEvent variants
- **WHEN** `grep -rn "TurnStarted\|TurnCompleted\|TurnFailed\|TurnAborted" crates/` is run
- **THEN** zero matches SHALL appear (the four variants SHALL NOT exist)

#### Scenario: No Turn struct, no TurnStatus enum
- **WHEN** `grep -rn "pub struct Turn\b\|pub enum TurnStatus" crates/` is run
- **THEN** zero matches SHALL appear
- **THEN** the only turn-related type defined SHALL be `TurnId`

### Requirement: The MVP SHALL depend on the three prerequisite tasks being complete

This change SHALL NOT be applied (thawed) until the following three prerequisite changes have been completed and archived:
1. `unify-token-usage-types` — collapse four `TokenUsage` definitions into one
2. `turn-id-unify` (not yet started) — collapse four `turn_id` representations (usize, u64, String×2) into one
3. `recovery-path-explicit` (not yet started) — make `RecoveryResult::Recovered` branch in `builder.rs:355-363` emit explicit end-of-iteration events

#### Scenario: Prerequisite gating
- **WHEN** evaluating whether to thaw this change
- **THEN** `openspec list` SHALL show `unify-token-usage-types` as `archived`
- **THEN** `openspec list` SHALL show `turn-id-unify` as `archived` (or equivalent capability)
- **THEN** `openspec list` SHALL show `recovery-path-explicit` as `archived` (or equivalent capability)
- **THEN** if any prerequisite is missing, the thaw SHALL be deferred

#### Scenario: Six-month hard cap
- **WHEN** the calendar date reaches 2026-12-13 (six months after 2026-06-13) without meeting any thaw condition
- **THEN** this change SHALL be archived to `openspec/changes/archive/turn-id-mvp-expired/`
- **THEN** the `turn-id-label` capability SHALL be marked as "deferred indefinitely"
</content>
</invoke>