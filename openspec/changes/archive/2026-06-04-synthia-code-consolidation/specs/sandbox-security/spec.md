## MODIFIED Requirements

### Requirement: Sandbox Implementation Centralized in Exec
The sandbox implementation SHALL be canonical in `synthia-exec/src/sandbox.rs`. Guardian SHALL only perform policy checks, not implement sandbox itself.

#### Scenario: Sandbox execution via exec
- **WHEN** sandbox execution is needed
- **THEN** it SHALL use `synthia-exec/src/sandbox.rs`

#### Scenario: Guardian performs policy checks only
- **WHEN** guardian validates an operation
- **THEN** it SHALL perform policy checks without implementing its own sandbox

#### Scenario: Sandbox behavior unchanged
- **WHEN** sandbox is used for execution
- **THEN** the behavior SHALL be identical to before consolidation