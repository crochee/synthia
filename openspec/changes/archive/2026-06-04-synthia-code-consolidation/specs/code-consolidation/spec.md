## ADDED Requirements

### Requirement: Code consolidation SHALL eliminate duplicate ReAct implementations
The system SHALL consolidate all ReAct loop implementations so that exactly one canonical implementation exists at `synthia-agent/src/agent/react.rs`.

#### Scenario: ReAct consolidation completes
- **WHEN** the consolidation is complete
- **THEN** there SHALL be exactly one ReAct implementation in the codebase

### Requirement: Duplicate type definitions SHALL be unified
All duplicate definitions of `AgentEvent`, `AgentConfig`, `SessionConfig`, and similar core types SHALL be eliminated with a single canonical definition.

#### Scenario: AgentEvent unified
- **WHEN** checking for duplicate AgentEvent definitions
- **THEN** exactly one canonical definition SHALL exist at `synthia-agent/src/types/event.rs`

### Requirement: Orphan crates SHALL be evaluated and resolved
Crates present on disk but not listed in `[workspace.members]` SHALL be either deleted (if functionality is covered elsewhere) or migrated.

#### Scenario: Orphan crate evaluation complete
- **WHEN** all orphan crates have been evaluated
- **THEN** each crate SHALL be either deleted or added to workspace members

### Requirement: Registry implementations SHALL use core::Registry<T>
All hand-rolled registry implementations across crates SHALL be replaced with `core::Registry<T>`.

#### Scenario: Registry replacement verified
- **WHEN** verifying registry implementations
- **THEN** all registry types SHALL be type aliases or wrappers around `core::Registry<T>`