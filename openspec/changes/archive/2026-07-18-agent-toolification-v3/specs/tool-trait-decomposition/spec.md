## ADDED Requirements

### Requirement: Tool Sub-Trait Decomposition

The `Tool` trait SHALL be decomposed into three sub-traits: `ToolDefinition`, `ToolExecution`, and `ToolLifecycle`. Each sub-trait MUST expose at most 5 methods. A legacy `ToolV1` alias MUST be provided that aggregates all three sub-traits for backward compatibility.

#### Scenario: Definition Sub-Trait Shape

- **WHEN** a developer inspects `ToolDefinition`
- **THEN** it SHALL expose at most 5 methods covering: `name()`, `description()`, `parameters_schema()`, `category()`, and `to_metadata()`

#### Scenario: Execution Sub-Trait Shape

- **WHEN** a developer inspects `ToolExecution`
- **THEN** it SHALL expose at most 5 methods covering: `execute()`, `validate()`, `dry_run()`, `cost_estimate()`, and `cancel()`

#### Scenario: Lifecycle Sub-Trait Shape

- **WHEN** a developer inspects `ToolLifecycle`
- **THEN** it SHALL expose at most 5 methods covering: `on_register()`, `on_unregister()`, `health_check()`, `version()`, and `schema_version()`

#### Scenario: Backward Compatibility Alias

- **WHEN** existing code references the original `Tool` trait
- **THEN** the `ToolV1` alias SHALL compile and produce equivalent behavior for at least 2 minor versions