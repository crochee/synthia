# agent-react-loop Specification

## Purpose
Marks the legacy `ReActLoop` struct as `#[deprecated]` in favour of `StreamBuilder` as the production
implementation, and migrates external consumers (e2e tests, downstream crates) to test behaviour
against `StreamBuilder` or direct behaviour assertions rather than the deprecated struct.
## Requirements
### Requirement: ReActLoop struct SHALL be marked deprecated

The `ReActLoop` struct in `react.rs` SHALL carry a `#[deprecated]` attribute with a message indicating `StreamBuilder` is the production implementation.

#### Scenario: Compiler emits deprecation warning
- **WHEN** code references `ReActLoop` directly
- **THEN** the compiler SHALL emit a deprecation warning

### Requirement: Deprecated ReActLoop consumers SHALL be migrated

Any external consumer of `ReActLoop` (such as `synthia-e2e/reasoning_tracking.rs`) SHALL be migrated to test behavior against `StreamBuilder` or test the intended behavior directly rather than the deprecated struct.

#### Scenario: E2E test migrated away from ReActLoop
- **WHEN** `synthia-e2e/reasoning_tracking.rs` is updated
- **THEN** it SHALL test reasoning tracking behavior via `StreamBuilder` or direct behavior assertions

