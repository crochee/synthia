<!--
Synced spec for turn-id-mvp-thaw-eval-2026-06-13 (cumulative format).
Source: openspec/changes/turn-id-mvp-thaw-eval-2026-06-13/specs/turn-id-mvp-thaw-eval-2026-06-13/spec.md
Synced: 2026-06-13
-->

# turn-id-mvp-thaw-eval-2026-06-13 Specification

## Purpose
Record the 3/3 prerequisite completion event on 2026-06-13 mid-freeze
(`unify-token-usage-types` archived 2026-06-12 + `turn-id-unify` archived
2026-06-13 + `recovery-path-explicit` archived 2026-06-13 as
`explicit-recovery-paths`) as the evidence that the **implementation
prerequisites** for the `turn-id-mvp` change are now complete, but the
3-month freeze period (2026-06-13 → 2026-09-13) is maintained
unchanged. This change re-evaluates the freeze in light of that
evidence plus codex v0.129 (2026-05-08) and v0.140 alpha (2026-06-10)
incremental signals, and formalizes the 4-party unanimous 0-thaw
decision. This change is a meta-change: it introduces zero code
modifications and SHALL NOT modify the FROZEN `turn-id-mvp` directory.

## Requirements

### Requirement: The 3/3 prerequisite completion event SHALL be recorded in proposal.md and design.md

The `openspec/changes/turn-id-mvp-thaw-eval-2026-06-13/proposal.md` and `design.md` SHALL each cite the three prerequisite change names that completed on or before 2026-06-13 mid-freeze: `unify-token-usage-types` (archived 2026-06-12), `turn-id-unify` (archived 2026-06-13), and `recovery-path-explicit` (archived 2026-06-13 as `explicit-recovery-paths`). The proposal SHALL quote at least one verbatim phrase from the `turn-id-unify/retrospective.md` "3/3 prerequisites spec-complete" follow-up entry.

#### Scenario: All three prerequisite change names cited
- **WHEN** `openspec/changes/turn-id-mvp-thaw-eval-2026-06-13/proposal.md` is read
- **THEN** it SHALL mention all three change names: `unify-token-usage-types`, `turn-id-unify`, and `recovery-path-explicit` (or its manifestation `explicit-recovery-paths`)
- **AND** SHALL record the archive date of each prerequisite

#### Scenario: turn-id-unify retrospective quote recorded
- **WHEN** `openspec/changes/turn-id-mvp-thaw-eval-2026-06-13/proposal.md` is read
- **THEN** it SHALL include a verbatim quote or near-verbatim reference to the `turn-id-unify/retrospective.md` follow-up entry text containing the phrase "3/3 spec-complete" or "3/3 prerequisites"

#### Scenario: design.md prerequisite list recorded
- **WHEN** `openspec/changes/turn-id-mvp-thaw-eval-2026-06-13/design.md` Context section is read
- **THEN** it SHALL list all three prerequisite change names with their archive dates
- **AND** SHALL state that the prerequisites are now 3/3 complete (spec+code)

### Requirement: The three-month freeze period SHALL NOT be shortened based on 3/3 prerequisite completion

The 3-month freeze period from 2026-06-13 to 2026-09-13 for the `turn-id-mvp` change SHALL remain in force after this change. The 3/3 prerequisite completion event (satisfying the **implementation** prerequisite, not a **thaw trigger** condition) SHALL NOT trigger an immediate thaw or a shortened freeze window. The 3 prerequisite changes are the **implementation** prerequisite per `turn-id-mvp/proposal.md`, distinct from the **thaw** conditions per `turn-id-mvp/design.md` D2 (concrete caller + other primitives + 3-month window).

#### Scenario: Freeze end date unchanged
- **WHEN** the calendar date reaches any point between 2026-06-13 and 2026-09-13 inclusive
- **THEN** the `turn-id-mvp` change SHALL remain in FROZEN state
- **AND** `openspec list` SHALL show `turn-id-mvp` as frozen (not in-progress, not archived)

#### Scenario: Decision recorded in design.md
- **WHEN** `openspec/changes/turn-id-mvp-thaw-eval-2026-06-13/design.md` Decisions section is read
- **THEN** it SHALL contain a decision (labeled D3 or equivalent) explicitly stating that the 3-month freeze period is maintained without shortening
- **AND** SHALL cite at least 3 of the 4 reasons: "3/3 prerequisites complete is implementation prerequisite not thaw trigger", "codex v0.129 uses usize not Uuid", "codex v0.140 alpha is not GA", "0 production caller in Synthia"

#### Scenario: Rationale for not shortening is documented
- **WHEN** `openspec/changes/turn-id-mvp-thaw-eval-2026-06-13/design.md` is read
- **THEN** at least one Risks/Trade-offs entry SHALL explain the "implementation prerequisite ≠ thaw trigger" rationale
- **AND** SHALL distinguish implementation prerequisites from thaw trigger conditions

#### Scenario: 4-party consensus recorded
- **WHEN** `openspec/changes/turn-id-mvp-thaw-eval-2026-06-13/brainstorm.md` is read
- **THEN** it SHALL contain a 4-party review table showing 怀疑派 / 架构派 / 生产派 / 简化派 positions
- **AND** SHALL show all 4 parties voting to maintain the freeze (4-0 unanimous)

### Requirement: codex v0.129 and v0.140 incremental signals SHALL be recorded but SHALL NOT constitute thaw evidence

The `openspec/changes/turn-id-mvp-thaw-eval-2026-06-13/proposal.md` and `design.md` SHALL each cite the two incremental codex signals that emerged between 2026-05-08 and 2026-06-10: codex CLI v0.129 (session picker exposes "Turn count" as `usize` counter, not `Uuid`) and codex CLI v0.140 alpha (multi-agent v2 path tracking, not yet GA, type of turn ID not confirmed as `Uuid`). The proposal SHALL explicitly state that v0.129's `usize` practice is **consistent** with the current Synthia `LoopContext.iteration: usize` design and v0.140 alpha's multi-agent path tracking is **not yet** industrial evidence for `TurnId(Uuid)`.

#### Scenario: codex v0.129 recorded with usize note
- **WHEN** `openspec/changes/turn-id-mvp-thaw-eval-2026-06-13/proposal.md` is read
- **THEN** it SHALL mention `codex CLI v0.129` with date 2026-05-08
- **AND** SHALL state that v0.129 exposes turn count as `usize` (not `Uuid`)

#### Scenario: codex v0.140 alpha recorded with uncertainty
- **WHEN** `openspec/changes/turn-id-mvp-thaw-eval-2026-06-13/proposal.md` is read
- **THEN** it SHALL mention `codex CLI v0.140 alpha` with date 2026-06-10
- **AND** SHALL state that v0.140 alpha's multi-agent path tracking is not yet GA
- **AND** SHALL state that the type of turn ID in v0.140 multi-agent is not confirmed as `Uuid`

#### Scenario: design.md decision on codex signals
- **WHEN** `openspec/changes/turn-id-mvp-thaw-eval-2026-06-13/design.md` Decisions section is read
- **THEN** it SHALL contain a decision (labeled D2 or equivalent) explicitly stating that codex v0.129 + v0.140 incremental signals do NOT constitute thaw evidence
- **AND** SHALL record the 3-month observation window value (waiting for v0.140 GA)

### Requirement: TurnId MVP implementation SHALL remain gated by the 3-month freeze period

This change SHALL NOT enable implementation of the TurnId MVP. The actual implementation of `turn-id-mvp` SHALL remain blocked until 2026-09-13 (the freeze end date). Even with 3/3 implementation prerequisites complete, the freeze period itself SHALL remain the controlling gate. The 3 prerequisite changes (`unify-token-usage-types` / `turn-id-unify` / `recovery-path-explicit`) SHALL continue to be in archived state — this change SHALL NOT modify or un-archive any of them.

#### Scenario: Prerequisites remain in archived state
- **WHEN** this change is applied
- **THEN** `openspec list` SHALL continue to show `unify-token-usage-types` in archived state
- **AND** `openspec list` SHALL continue to show `turn-id-unify` in archived state
- **AND** `openspec list` SHALL continue to show `recovery-path-explicit` in archived state

#### Scenario: Freeze end date remains the controlling gate
- **WHEN** this change is applied
- **THEN** the 3-month freeze period (2026-06-13 → 2026-09-13) SHALL remain the controlling gate for `turn-id-mvp` implementation
- **AND** no documentation in this change SHALL claim that the freeze has been lifted or shortened

#### Scenario: No thaw criteria met prematurely
- **WHEN** this change is applied
- **THEN** no document in `openspec/changes/turn-id-mvp-thaw-eval-2026-06-13/` SHALL claim that any of the 3 thaw conditions (concrete caller + other primitives + 3-month window) from `turn-id-mvp/design.md` D2 have been met
- **AND** the 3-month window condition SHALL be the only one that may be met by passage of time alone

### Requirement: codex Turn design SHALL be treated as reference only, not copied

If and when `turn-id-mvp` is eventually thawed and implemented, the codex Turn design (including but not limited to v0.129's session picker "Turn count" exposure and v0.140 alpha's multi-agent path tracking) SHALL serve as reference material for understanding industrial-grade Turn semantics, but no code SHALL be copied from codex into the Synthia codebase. Synthia SHALL continue to follow the simplified MVP path (~20 lines, single `TurnId(Uuid)` type, no `Turn` struct, no `TurnStatus` enum, no new `AgentEvent` variants, no persistence).

#### Scenario: Reference-only stance recorded
- **WHEN** `openspec/changes/turn-id-mvp-thaw-eval-2026-06-13/design.md` Decisions section is read
- **THEN** it SHALL contain a decision (labeled D5 or equivalent) stating that codex Turn design is reference-only and SHALL NOT be copied

#### Scenario: No codex modules imported
- **WHEN** the Synthia workspace is searched with `grep -rn "use codex" crates/`
- **THEN** zero matches SHALL appear after this change is applied

#### Scenario: MVP scope preserved
- **WHEN** `openspec/changes/turn-id-mvp-thaw-eval-2026-06-13/design.md` is read
- **THEN** it SHALL state that the Synthia TurnId MVP SHALL be limited to a single `TurnId(Uuid)` type with no `Turn` struct, no `TurnStatus` enum, no new `AgentEvent` variants, and no persistence layer

### Requirement: The FROZEN turn-id-mvp directory SHALL NOT be modified by this change

This change SHALL NOT modify, edit, add, or delete any file under `openspec/changes/turn-id-mvp/`. The FROZEN state of `turn-id-mvp` SHALL be preserved exactly as it existed on 2026-06-13 (when the freeze was first imposed).

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

This change is a meta-change and SHALL NOT modify any file outside of `openspec/changes/turn-id-mvp-thaw-eval-2026-06-13/`. In particular, no file under `crates/`, `tools/`, `scripts/`, or any other source directory SHALL be created, modified, or deleted by this change. The `openspec/changes/archive/2026-06-13-turn-id-unify/retrospective.md` follow-up entry updating "2/3 prerequisites" to "3/3 spec-complete" (committed in this session) is the only file outside this change directory that may have been modified, and SHALL be the last such change.

#### Scenario: Only OpenSpec artifacts created
- **WHEN** `git status` is run after this change is applied
- **THEN** the only newly created files SHALL be under `openspec/changes/turn-id-mvp-thaw-eval-2026-06-13/`

#### Scenario: No source code changes
- **WHEN** `git diff --stat` is run with a filter for `crates/`, `tools/`, `scripts/`, `tests/`
- **THEN** the output SHALL be empty (no new files, no modifications)

#### Scenario: No turn_id type changes
- **WHEN** `grep -rn "TurnId\|current_turn_id" crates/` is run after this change is applied
- **THEN** no new matches SHALL appear in the output (no new type definitions, no new field assignments beyond what `turn-id-unify` already introduced)

#### Scenario: No new AgentEvent variants
- **WHEN** `grep -rn "TurnStarted\|TurnCompleted\|TurnFailed\|TurnAborted" crates/` is run after this change is applied
- **THEN** the output SHALL remain at zero matches (no new event variants added)

### Requirement: This change SHALL pass openspec validation

After all 8 artifacts (`.openspec.yaml` + `README.md` + `brainstorm.md` + `design.md` + `proposal.md` + `specs/turn-id-mvp-thaw-eval-2026-06-13/spec.md` + `tasks.md` + `plan.md` + `verify.md` + `retrospective.md`) are created, the command `openspec validate turn-id-mvp-thaw-eval-2026-06-13 --type change` SHALL exit with status 0 and SHALL NOT report any validation errors or warnings. The command `openspec validate turn-id-mvp-thaw-eval-2026-06-13 --type change --strict` SHALL also exit with status 0.

#### Scenario: Standard validation passes
- **WHEN** `openspec validate turn-id-mvp-thaw-eval-2026-06-13 --type change` is executed
- **THEN** the exit code SHALL be 0
- **AND** the output SHALL contain the text "Change 'turn-id-mvp-thaw-eval-2026-06-13' is valid" (or equivalent success message)

#### Scenario: Strict validation passes
- **WHEN** `openspec validate turn-id-mvp-thaw-eval-2026-06-13 --type change --strict` is executed
- **THEN** the exit code SHALL be 0
- **AND** no validation warnings or errors SHALL be reported

#### Scenario: All requirements have at least one scenario
- **WHEN** `openspec/changes/turn-id-mvp-thaw-eval-2026-06-13/specs/turn-id-mvp-thaw-eval-2026-06-13/spec.md` is read
- **THEN** every `### Requirement:` heading SHALL be followed by at least one `#### Scenario:` heading
- **AND** no Requirement SHALL exist without an associated Scenario

#### Scenario: First sentence contains SHALL or MUST
- **WHEN** `openspec/changes/turn-id-mvp-thaw-eval-2026-06-13/specs/turn-id-mvp-thaw-eval-2026-06-13/spec.md` is read
- **THEN** the first sentence of every Requirement's body (the text before the first period or newline after the Requirement heading) SHALL contain either the word "SHALL" or the word "MUST"
