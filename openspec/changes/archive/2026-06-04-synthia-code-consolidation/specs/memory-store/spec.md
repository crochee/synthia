## MODIFIED Requirements

### Requirement: MemoryStore Trait Split by Operation Type
The `MemoryStore` trait SHALL be split into read and write sub-traits defined in `synthia-memory/src/types.rs`. File store implements read operations, cold store implements write operations.

#### Scenario: Read operations via file_store
- **WHEN** reading from memory store
- **THEN** it SHALL use `file_store.rs` implementation

#### Scenario: Write operations via cold_store
- **WHEN** writing to memory store
- **THEN** it SHALL use `cold/store.rs` implementation

#### Scenario: Unified access interface
- **WHEN** code needs to access memory
- **THEN** it SHALL use the unified `MemoryStore` trait interface