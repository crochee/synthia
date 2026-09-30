<!--
Delta spec for the turn-id-unfreeze meta-change.

This change is a META-change: it records the codex PR #28002 + #27996
unfreeze trigger for the FROZEN turn-id-mvp change, re-evaluates the
3-month freeze period, and formalizes the decision — without itself
introducing any code changes. All TurnId MVP implementation remains
gated by the original three prerequisites from turn-id-mvp.

Per OpenSpec rules:
- This is an ADDED-only delta (no existing turn-id-unfreeze spec to modify).
- Each Requirement's first sentence MUST contain SHALL or MUST.
- Each Scenario MUST use level-4 heading (`####`) with WHEN/THEN format.
- Every Requirement MUST have at least one Scenario.
-->

# turn-id-unfreeze Specification

## Purpose
Record the OpenAI codex PR #28002 + #27996 trigger event (both merged
2026-06-13) as the concrete-use-case evidence that satisfies the
`turn-id-mvp` change's first unfreeze condition, re-evaluate the
3-month freeze period in light of that new evidence, and formalize
the decision to maintain the freeze period without shortening it.
This change is a meta-change: it introduces zero code modifications
and SHALL NOT modify the FROZEN `turn-id-mvp` directory.

## ADDED Requirements

### Requirement: Unfreeze trigger evidence SHALL be recorded in proposal.md and design.md

The `openspec/changes/turn-id-unfreeze/proposal.md` and `design.md` SHALL each cite the two OpenAI codex pull requests that triggered the unfreeze evaluation: codex PR #28002 `[codex] Send turn state through compact requests` (modifying `codex-rs/core/src/session/turn.rs`) and codex PR #27996 `[codex] Send request-scoped turn state over WebSocket` (also modifying `codex-rs/core/src/session/turn.rs`). The proposal SHALL quote the relevant description text from PR #27996 verbatim.

#### Scenario: codex PR numbers cited
- **WHEN** `openspec/changes/turn-id-unfreeze/proposal.md` is read
- **THEN** it SHALL mention both `codex PR #28002` and `codex PR #27996`
- **AND** SHALL reference the file path `codex-rs/core/src/session/turn.rs`

#### Scenario: PR #27996 description quoted
- **WHEN** `openspec/changes/turn-id-unfreeze/proposal.md` is read
- **THEN** it SHALL include a verbatim quote from PR #27996 containing the phrase "Turn state is scoped to one logical turn"

#### Scenario: codex module footprint recorded
- **WHEN** `openspec/changes/turn-id-unfreeze/design.md` is read
- **THEN** it SHALL list at least 5 of the following 6 codex Turn-related modules: `codex-rs/core/src/session/turn.rs` (2296 lines), `codex-rs/core/src/turn_timing.rs` (391 lines), `codex-rs/core/src/turn_metadata.rs` (349 lines), `codex-rs/core/src/turn_diff_tracker.rs`, `codex-rs/core/src/state/turn.rs` (241 lines), `codex-rs/core/src/context/turn_aborted.rs`

### Requirement: The three-month freeze period SHALL NOT be shortened

The 3-month freeze period from 2026-06-13 to 2026-09-13 for the `turn-id-mvp` change SHALL remain in force after this change. The codex PR #28002 + #27996 evidence (satisfying unfreeze condition #1) SHALL NOT trigger an immediate unfreeze or a shortened freeze window.

#### Scenario: Freeze end date unchanged
- **WHEN** the calendar date reaches any point between 2026-06-13 and 2026-09-13 inclusive
- **THEN** the `turn-id-mvp` change SHALL remain in FROZEN state
- **AND** `openspec list` SHALL show `turn-id-mvp` as frozen (not in-progress, not archived)

#### Scenario: Decision recorded in design.md
- **WHEN** `openspec/changes/turn-id-unfreeze/design.md` Decisions section is read
- **THEN** it SHALL contain a decision (labeled D2 or equivalent) explicitly stating that the 3-month freeze period is maintained without shortening
- **AND** SHALL cite at least 2 reasons from: "preserves project principle of deferring speculative architecture", "3 prerequisite changes still incomplete", "preserves the 3-month observation window value"

#### Scenario: Rationale for not shortening is documented
- **WHEN** `openspec/changes/turn-id-unfreeze/design.md` is read
- **THEN** at least one Risks/Trade-offs entry SHALL explain the "external trigger should not collapse internal freeze" rationale

### Requirement: TurnId MVP implementation SHALL remain gated by the three prerequisite changes

This change SHALL NOT enable implementation of the TurnId MVP. The actual implementation of `turn-id-mvp` SHALL remain blocked until all three prerequisite changes are archived: (a) `unify-token-usage-types`, (b) `turn-id-unify`, (c) `recovery-path-explicit`. None of these prerequisites SHALL be marked as completed by this change.

#### Scenario: Prerequisites are not bypassed
- **WHEN** this change is applied
- **THEN** `openspec list` SHALL continue to show `unify-token-usage-types` in its prior state (not artificially archived)
- **AND** `openspec list` SHALL continue to show `turn-id-unify` in its prior state (not started)
- **AND** `openspec list` SHALL continue to show `recovery-path-explicit` in its prior state (not started)

#### Scenario: Prerequisite gating recorded in design.md
- **WHEN** `openspec/changes/turn-id-unfreeze/design.md` Decisions section is read
- **THEN** it SHALL contain a decision (labeled D4 or equivalent) explicitly listing all three prerequisite change names

#### Scenario: Prerequisite gating recorded in spec
- **WHEN** `openspec/changes/turn-id-unfreeze/specs/turn-id-unfreeze/spec.md` is read
- **THEN** it SHALL contain at least one Requirement stating that TurnId MVP implementation depends on completion of the three prerequisite changes

### Requirement: codex Turn design SHALL be treated as reference only, not copied

If and when `turn-id-mvp` is eventually thawed and implemented, the codex Turn design (including but not limited to `codex-rs/core/src/session/turn.rs`, `turn_metadata.rs`, `turn_timing.rs`, `state/turn.rs`, `turn_diff_tracker.rs`, `context/turn_aborted.rs`) SHALL serve as reference material for understanding industrial-grade Turn semantics, but no code SHALL be copied from codex into the Synthia codebase. Synthia SHALL continue to follow the simplified MVP path (~20 lines, single `TurnId(Uuid)` type, no `Turn` struct, no `TurnStatus` enum, no new `AgentEvent` variants, no persistence).

#### Scenario: Reference-only stance recorded
- **WHEN** `openspec/changes/turn-id-unfreeze/design.md` Decisions section is read
- **THEN** it SHALL contain a decision (labeled D3 or equivalent) stating that codex Turn design is reference-only and SHALL NOT be copied

#### Scenario: No codex modules imported
- **WHEN** the Synthia workspace is searched with `grep -rn "use codex" crates/`
- **THEN** zero matches SHALL appear after this change is applied

#### Scenario: MVP scope preserved
- **WHEN** `openspec/changes/turn-id-unfreeze/design.md` is read
- **THEN** it SHALL state that the Synthia TurnId MVP SHALL be limited to a single `TurnId(Uuid)` type with no `Turn` struct, no `TurnStatus` enum, no new `AgentEvent` variants, and no persistence layer

### Requirement: The FROZEN turn-id-mvp directory SHALL NOT be modified by this change

This change SHALL NOT modify, edit, add, or delete any file under `openspec/changes/turn-id-mvp/`. The FROZEN state of `turn-id-mvp` SHALL be preserved exactly as it existed on 2026-06-13.

#### Scenario: turn-id-mvp files unchanged
- **WHEN** `git diff --stat openspec/changes/turn-id-mvp/` is run after this change is applied
- **THEN** the output SHALL be empty (no files added, modified, or deleted)

#### Scenario: FROZEN marker preserved
- **WHEN** `openspec/changes/turn-id-mvp/proposal.md` is read after this change is applied
- **THEN** it SHALL still contain a "FROZEN" marker in the Why section
- **AND** SHALL still state the freeze period 2026-06-13 → 2026-09-13

#### Scenario: FROZEN state in spec preserved
- **WHEN** `openspec/changes/turn-id-mvp/specs/turn-id-label/spec.md` is read after this change is applied
- **THEN** it SHALL still contain a Requirement stating that the change is FROZEN from 2026-06-13 to 2026-09-13
- **AND** SHALL NOT contain any text indicating the freeze has been lifted

### Requirement: This change SHALL introduce zero code changes to the Synthia codebase

This change is a meta-change and SHALL NOT modify any file outside of `openspec/changes/turn-id-unfreeze/`. In particular, no file under `crates/`, `tools/`, `scripts/`, or any other source directory SHALL be created, modified, or deleted by this change.

#### Scenario: Only OpenSpec artifacts created
- **WHEN** `git status` is run after this change is applied
- **THEN** the only newly created files SHALL be under `openspec/changes/turn-id-unfreeze/`
- **AND** SHALL consist of exactly 4 files: `proposal.md`, `design.md`, `tasks.md`, and `specs/turn-id-unfreeze/spec.md`

#### Scenario: No source code changes
- **WHEN** `git diff --stat` is run with a filter for `crates/`, `tools/`, `scripts/`, `tests/`
- **THEN** the output SHALL be empty

#### Scenario: No turn_id type changes
- **WHEN** `grep -rn "TurnId\|current_turn_id" crates/` is run after this change is applied
- **THEN** no new matches SHALL appear in the output (no new type definitions, no new field assignments)

#### Scenario: No new AgentEvent variants
- **WHEN** `grep -rn "TurnStarted\|TurnCompleted\|TurnFailed\|TurnAborted" crates/` is run after this change is applied
- **THEN** the output SHALL remain at zero matches (no new event variants added)

### Requirement: This change SHALL pass openspec validation

After all 4 artifacts (`proposal.md`, `design.md`, `tasks.md`, `specs/turn-id-unfreeze/spec.md`) are created, the command `openspec validate turn-id-unfreeze --type change` SHALL exit with status 0 and SHALL NOT report any validation errors or warnings. The command `openspec validate turn-id-unfreeze --type change --strict` SHALL also exit with status 0.

#### Scenario: Standard validation passes
- **WHEN** `openspec validate turn-id-unfreeze --type change` is executed
- **THEN** the exit code SHALL be 0
- **AND** the output SHALL contain the text "Change 'turn-id-unfreeze' is valid" (or equivalent success message)

#### Scenario: Strict validation passes
- **WHEN** `openspec validate turn-id-unfreeze --type change --strict` is executed
- **THEN** the exit code SHALL be 0
- **AND** no validation warnings or errors SHALL be reported

#### Scenario: All requirements have at least one scenario
- **WHEN** `openspec/changes/turn-id-unfreeze/specs/turn-id-unfreeze/spec.md` is read
- **THEN** every `### Requirement:` heading SHALL be followed by at least one `#### Scenario:` heading
- **AND** no Requirement SHALL exist without an associated Scenario

#### Scenario: First sentence contains SHALL or MUST
- **WHEN** `openspec/changes/turn-id-unfreeze/specs/turn-id-unfreeze/spec.md` is read
- **THEN** the first sentence of every Requirement's body (the text before the first period or newline after the Requirement heading) SHALL contain either the word "SHALL" or the word "MUST"
