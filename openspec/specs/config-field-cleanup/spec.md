# config-field-cleanup Specification

## Purpose
TBD - created by archiving change agent-toolification-v3. Update Purpose after archive.
## Requirements
### Requirement: Eliminate Underscore-Prefixed Silent-Drop Fields

The `AgentRunConfig` type MUST NOT contain fields whose identifier starts with `_` and that are read-only / dropped during agent execution. Every field MUST be either: (a) actively read during execution, (b) explicitly deleted with a migration note in CHANGELOG, or (c) renamed to its non-prefixed name with documented semantics.

#### Scenario: Field Audit

- **WHEN** a developer inspects `AgentRunConfig`
- **THEN** the type MUST contain zero fields starting with `_` after this change is applied

#### Scenario: Field Has Documented Lifecycle

- **WHEN** a field is renamed rather than deleted
- **THEN** the CHANGELOG entry MUST document the old name, the new name, and any consumer migration steps

### Requirement: Silent-Drop Detection Test

A unit test MUST exist that fails the build if any field in `AgentRunConfig` has an identifier starting with `_` and is not annotated with `#[allow(dead_code)]` paired with a `// reason:` comment.

#### Scenario: CI Guard Against Silent Drop

- **WHEN** a new `_xxx` field is added to `AgentRunConfig` without justification
- **THEN** the test SHALL fail and the CI build MUST break

### Requirement: Backward Compatibility for Renamed Fields

When a field is renamed, the OLD name MUST remain available as a deprecated alias for at least one minor version, with `#[deprecated]` attribute pointing to the new name.

#### Scenario: Deprecation Warning

- **WHEN** existing code accesses a renamed field by its old name
- **THEN** the compiler MUST emit a `#[deprecated]` warning that names the new field

