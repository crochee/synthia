## MODIFIED Requirements

### Requirement: Registry Uses core::Registry<T>
All registry implementations across crates SHALL use `core::Registry<T>` as their underlying implementation.

#### Scenario: Registry implementations use core
- **WHEN** any crate needs registry functionality
- **THEN** it SHALL use `core::Registry<T>`

#### Scenario: Custom registry behavior via composition
- **WHEN** a crate needs additional registry behavior beyond core
- **THEN** it SHALL use `pub struct XxxRegistry { inner: Registry<T> }` pattern

#### Scenario: Registry API compatibility
- **WHEN** existing code uses registry
- **THEN** the API SHALL remain compatible after replacement with core::Registry<T>