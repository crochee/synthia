<!--
Delta spec for turn-id-unify change.
Capability: turn-id-unify (new, minimal viable convergence of 4 turn_id representations)
-->

# turn-id-unify Specification

## Purpose
Implement the minimum viable convergence of 4 existing `turn_id` representations in the Synthia codebase: (a) centralize the `format!("turn-{}", iter)` string construction used by `AgentContext.turn_id` into a single `synthia_agent::turn_id::format_turn_id(iter: usize) -> String` function, and (b) delete the orphan `turn_id: String` field from `ApprovalRequest::NetworkAccess` (one of 5 `ApprovalRequest` variants, with zero production callers and no Guardian decision logic that reads it). This change introduces ZERO new types and is the second of three orthogonal prerequisites for thawing the FROZEN `turn-id-mvp` change.

## Requirements

### Requirement: turn_id string construction SHALL be centralized in synthia_agent::turn_id::format_turn_id

The string construction `format!("turn-{}", iter)` used to populate `AgentContext.turn_id: String` in `crates/synthia-agent/src/stream_builder/builder.rs` SHALL be centralized into a single function `synthia_agent::turn_id::format_turn_id(iter: usize) -> String` defined in a new file `crates/synthia-agent/src/turn_id.rs`. The function SHALL return the string `"turn-{iter}"` (e.g., `format_turn_id(0) == "turn-0"`, `format_turn_id(42) == "turn-42"`).

#### Scenario: Helper function exists at canonical path
- **WHEN** the file `crates/synthia-agent/src/turn_id.rs` is read
- **THEN** it SHALL define a `pub fn format_turn_id(iter: usize) -> String` function
- **AND** the function body SHALL be `format!("turn-{}", iter)`

#### Scenario: format_turn_id produces correct output
- **WHEN** `format_turn_id(0)` is called
- **THEN** the result SHALL equal the string `"turn-0"`
- **WHEN** `format_turn_id(1)` is called
- **THEN** the result SHALL equal the string `"turn-1"`
- **WHEN** `format_turn_id(42)` is called
- **THEN** the result SHALL equal the string `"turn-42"`

#### Scenario: Module is publicly exported
- **WHEN** `crates/synthia-agent/src/lib.rs` is read
- **THEN** it SHALL contain `pub mod turn_id;`

#### Scenario: StreamBuilder calls the helper function
- **WHEN** `crates/synthia-agent/src/stream_builder/builder.rs` is read
- **THEN** the call site at line 360 (`AgentContext::new(..., turn_id, ...)`) SHALL pass `crate::turn_id::format_turn_id(ctx.iteration)` as the `turn_id` argument
- **AND** the literal `format!("turn-{}", ...)` SHALL NOT appear in this file (search restricted to `stream_builder/builder.rs`)

#### Scenario: No other format!("turn-{}", ...) literals remain
- **WHEN** `grep -rn 'format!("turn-{}"' crates/synthia-agent/` is run
- **THEN** the only match SHALL be inside `crates/synthia-agent/src/turn_id.rs` (the helper function body)
- **AND** zero matches SHALL appear in any other file under `crates/synthia-agent/`

### Requirement: ApprovalRequest::NetworkAccess SHALL NOT carry a turn_id field

The `ApprovalRequest::NetworkAccess` variant in `crates/synthia-guardian/src/approval_request.rs` SHALL NOT contain a `turn_id: String` field. The associated `ApprovalRequest::network_access(id, target, host, protocol, port)` constructor SHALL take 5 parameters (not 6). The `turn_id` field removal is a breaking API change; project-internal grep SHALL confirm zero production callers use the 6-parameter form.

#### Scenario: NetworkAccess variant has no turn_id field
- **WHEN** `crates/synthia-guardian/src/approval_request.rs` is read
- **THEN** the `NetworkAccess` variant SHALL be defined as `NetworkAccess { id: String, target: String, host: String, protocol: String, port: u16 }`
- **AND** the `NetworkAccess` variant SHALL NOT contain any `turn_id` field

#### Scenario: network_access constructor takes 5 parameters
- **WHEN** `crates/synthia-guardian/src/approval_request.rs` is read
- **THEN** the `pub fn network_access(...)` function SHALL take exactly 5 parameters: `id`, `target`, `host`, `protocol`, `port`
- **AND** the function SHALL NOT take a `turn_id` parameter

#### Scenario: Zero production callers of 6-parameter form
- **WHEN** `grep -rn 'ApprovalRequest::network_access' crates/ --include='*.rs' | grep -v test` is run
- **THEN** zero matches SHALL appear
- **WHEN** `grep -rn '\.network_access(' crates/ --include='*.rs'` is run
- **THEN** all matches SHALL use the 5-parameter form (no `turn_id` argument)

#### Scenario: NetworkAccess.turn_id field absence is preserved
- **WHEN** `grep -rn 'NetworkAccess.*{.*turn_id' crates/ --include='*.rs'` is run
- **THEN** zero matches SHALL appear (no struct literal references the removed field)

### Requirement: LoopContext.iteration SHALL retain its usize type

`crates/synthia-agent/src/loop_context.rs` SHALL retain the `pub iteration: usize` field unchanged. This change SHALL NOT modify the type, name, or initialization of the `iteration` field. The `should_reflect()` method's dependency on `iteration.is_multiple_of(5)` SHALL remain intact.

#### Scenario: iteration field type unchanged
- **WHEN** `crates/synthia-agent/src/loop_context.rs` is read
- **THEN** `LoopContext` SHALL contain `pub iteration: usize` (the type is `usize`, not `TurnId`, `u64`, or `String`)

#### Scenario: should_reflect logic unchanged
- **WHEN** `crates/synthia-agent/src/loop_context.rs:99` is read
- **THEN** the `should_reflect()` method SHALL contain `self.iteration > 0 && self.iteration.is_multiple_of(5)` (unchanged from pre-change state)

#### Scenario: No new turn_id field on LoopContext
- **WHEN** `grep -n 'current_turn_id\|turn_id' crates/synthia-agent/src/loop_context.rs` is run
- **THEN** zero matches SHALL appear (no `current_turn_id` or `turn_id` field is introduced by this change)

### Requirement: PrefixStabilityEvent.turn_id SHALL retain its u64 type

`crates/synthia-context/src/prefix_tracker.rs` SHALL retain the `pub turn_id: u64` field on `PrefixStabilityEvent` unchanged. This change SHALL NOT modify the type, name, or initialization of the `turn_id` field on `PrefixStabilityEvent`. The `PrefixTracker::record_pre(turn_id: u64)` and `PrefixTracker::emit_stability_event(turn_id: u64)` signatures SHALL remain unchanged.

#### Scenario: PrefixStabilityEvent.turn_id type unchanged
- **WHEN** `crates/synthia-context/src/prefix_tracker.rs` is read
- **THEN** `PrefixStabilityEvent` SHALL contain `pub turn_id: u64` (the type is `u64`, not `TurnId`, `usize`, or `String`)

#### Scenario: PrefixTracker signatures unchanged
- **WHEN** `crates/synthia-context/src/prefix_tracker.rs` is read
- **THEN** the `record_pre(system_bytes: &[u8], turn_id: u64) -> String` signature SHALL be unchanged
- **AND** the `emit_stability_event(turn_id: u64) -> PrefixStabilityEvent` signature SHALL be unchanged

#### Scenario: builder.rs prefix tracker call sites unchanged
- **WHEN** `crates/synthia-agent/src/stream_builder/builder.rs:376` is read
- **THEN** `prefix_tracker.lock().record_pre(&system_snapshot, ctx.iteration as u64)` SHALL remain unchanged
- **AND** the `as u64` cast SHALL remain (not changed to `format_turn_id` or `TurnId`)

### Requirement: This change SHALL introduce no new TurnId type

This change SHALL NOT introduce any new type named `TurnId`, `Turn`, `TurnStatus`, or any similar turn-related type. Specifically, the file `crates/synthia-agent/src/turn.rs` SHALL NOT be created (it remains for future use by the FROZEN `turn-id-mvp` change). The only file added by this change is `crates/synthia-agent/src/turn_id.rs` containing the `format_turn_id` helper function.

#### Scenario: No new TurnId type
- **WHEN** `grep -rn 'pub struct TurnId\|pub struct Turn\b\|pub enum TurnStatus' crates/` is run after this change
- **THEN** zero matches SHALL appear

#### Scenario: crates/synthia-agent/src/turn.rs is not created
- **WHEN** the file system is inspected
- **THEN** the file `crates/synthia-agent/src/turn.rs` SHALL NOT exist (reserved for future `turn-id-mvp` implementation)

#### Scenario: Only one new file in synthia-agent
- **WHEN** `git status crates/synthia-agent/` is run after this change is applied
- **THEN** the only newly created file SHALL be `crates/synthia-agent/src/turn_id.rs`
- **AND** no other file under `crates/synthia-agent/` SHALL be newly created

### Requirement: This change SHALL pass openspec validation and compilation

After all code modifications are complete, the following commands SHALL exit with status 0:
- `openspec validate turn-id-unify --type change` (OpenSpec metadata validation)
- `openspec validate turn-id-unify --type change --strict` (strict OpenSpec validation)
- `cargo check --workspace` (Rust compilation)
- `cargo test --workspace` (Rust test suite)
- `cargo +nightly fmt --all --check` (formatting check)
- `cargo clippy --all-targets --all-features --tests --all` (linter)

#### Scenario: OpenSpec validation passes
- **WHEN** `openspec validate turn-id-unify --type change` is executed
- **THEN** the exit code SHALL be 0
- **AND** the output SHALL contain "Change 'turn-id-unify' is valid"

#### Scenario: Strict OpenSpec validation passes
- **WHEN** `openspec validate turn-id-unify --type change --strict` is executed
- **THEN** the exit code SHALL be 0
- **AND** no validation warnings or errors SHALL be reported

#### Scenario: Cargo check passes
- **WHEN** `cargo check --workspace` is executed
- **THEN** the exit code SHALL be 0
- **AND** no compilation errors SHALL be reported

#### Scenario: Cargo test passes
- **WHEN** `cargo test --workspace` is executed
- **THEN** the exit code SHALL be 0
- **AND** all existing tests SHALL pass (no new failures introduced)

#### Scenario: Cargo fmt passes
- **WHEN** `cargo +nightly fmt --all --check` is executed
- **THEN** the exit code SHALL be 0
- **AND** no formatting diffs SHALL be reported

#### Scenario: Cargo clippy passes
- **WHEN** `cargo clippy --all-targets --all-features --tests --all` is executed
- **THEN** the exit code SHALL be 0
- **AND** no new clippy warnings SHALL be reported
