## ADDED Requirements

### Requirement: permission system structure verification
The refactored `Permission` enum (unified `PermissionLevel` and `PermissionDecision`) SHALL have a coherent structure with clear variant semantics.

#### Scenario: permission enum validation
- **WHEN** reviewing `synthia-permission/src/lib.rs` and related files
- **THEN** the reviewer SHALL confirm all permission variants are used consistently across the codebase

---

### Requirement: multi-agent残留 reference cleanup
All references to the deleted `synthia-multiagent` crate SHALL be removed or migrated.

#### Scenario: dead reference detection
- **WHEN** searching the codebase for `synthia-multiagent` imports or usages
- **THEN** no such references SHALL exist in any active code or test files

---

### Requirement: task scheduler responsibility boundary
The responsibilities of `TaskScheduler` in `synthia-agent/src/task/scheduler.rs` and `TaskDispatcher` in `synthia-task` SHALL be clearly delineated.

#### Scenario: responsibility boundary audit
- **WHEN** examining task scheduling and dispatching code paths
- **THEN** there SHALL be no duplicated logic or ambiguous responsibility assignment between the two components