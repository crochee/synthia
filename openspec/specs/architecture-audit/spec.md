# architecture-audit Specification

## Purpose
Audit of the refactored permission system, removal of `synthia-multiagent`残留 references, and the responsibility boundary between `TaskScheduler` and `TaskDispatcher`.
## Requirements
### Requirement: permission system structure verification
The refactored `Permission` enum (unified `PermissionLevel` and `PermissionDecision`) SHALL have a coherent structure with clear variant semantics. This requirement is **VERIFIED** as of 2026-07-12 against the current `synthia-permission` crate (see commit history from `2026-06-11-agent-bug-fix-and-dedup`).

#### Scenario: permission enum validation
- **WHEN** reviewing `synthia-permission/src/lib.rs` and related files
- **THEN** the reviewer SHALL confirm all permission variants are used consistently across the codebase

#### Scenario: permission enum structure verification (VERIFIED)
- **WHEN** grepping `crates/synthia-permission/src/lib.rs` for `PermissionLevel`, `PermissionDecision`, and `PermissionPolicy`
- **THEN** the three types SHALL form the single unified permission surface with no duplicate enum definitions or `PermissionPolicy` legacy types remaining — current code confirms this; marked VERIFIED

---

### Requirement: multi-agent残留 reference cleanup
All references to the deleted `synthia-multiagent` crate SHALL be removed or migrated. This requirement is **VERIFIED** as of 2026-07-12.

#### Scenario: dead reference detection
- **WHEN** searching the codebase for `synthia-multiagent` imports or usages
- **THEN** no such references SHALL exist in any active code or test files

#### Scenario: synthia-multiagent reference sweep (VERIFIED)
- **WHEN** running `grep -rn synthia-multiagent crates/`
- **THEN** the command SHALL return zero matches across active code and tests — confirmed; marked VERIFIED

---

### Requirement: task scheduler responsibility boundary
The responsibilities of `TaskScheduler` in `synthia-agent/src/task/scheduler.rs` and `TaskDispatcher` in `synthia-task` SHALL be clearly delineated. This requirement remains **OPEN** pending the design note referenced in Open Question 3.

#### Scenario: responsibility boundary audit
- **WHEN** examining task scheduling and dispatching code paths
- **THEN** there SHALL be no duplicated logic or ambiguous responsibility assignment between the two components

#### Scenario: TaskScheduler vs TaskDispatcher responsibility note (OPEN)
- **WHEN** the design note `design-notes/task-scheduler-task-dispatcher-boundary.md` lands at the path referenced in `design.md` Open Question 3
- **THEN** the note SHALL enumerate every shared responsibility and designate exactly one owner per concern — pending; will be marked VERIFIED once the note is committed