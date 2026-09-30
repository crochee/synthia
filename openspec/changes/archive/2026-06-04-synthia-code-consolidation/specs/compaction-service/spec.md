## MODIFIED Requirements

### Requirement: Compaction Service Unified in Context
The compaction logic SHALL be unified in `synthia-context/src/compaction/`. Agent and memory compaction SHALL call the context compaction implementation.

#### Scenario: Compaction called from agent
- **WHEN** agent requests compaction
- **THEN** it SHALL use `synthia-context/src/compaction/` implementation

#### Scenario: Compaction called from memory
- **WHEN** memory requests compaction
- **THEN** it SHALL use `synthia-context/src/compaction/` implementation

#### Scenario: No duplicate compaction implementations
- **WHEN** searching for compaction logic
- **THEN** only the context crate SHALL contain the canonical implementation