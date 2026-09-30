## MODIFIED Requirements

### Requirement: Checkpoint Service Unified in Context
The checkpoint/snapshot logic SHALL be unified in `synthia-context/src/checkpoint.rs`. Agent SHALL request snapshots, context SHALL handle persistence.

#### Scenario: Agent requests checkpoint
- **WHEN** agent requests a checkpoint
- **THEN** it SHALL delegate to `synthia-context/checkpoint.rs`

#### Scenario: Checkpoint persistence via context
- **WHEN** checkpoint needs to be persisted
- **THEN** context SHALL handle storage (not agent directly)

#### Scenario: No duplicate checkpoint implementations
- **WHEN** searching for checkpoint logic
- **THEN** only `synthia-context/checkpoint.rs` SHALL contain the canonical implementation