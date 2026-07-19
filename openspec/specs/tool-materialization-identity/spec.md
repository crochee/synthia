# Capability: tool-materialization-identity

> **Status**: Proposed (change #1: 架构基础设施)
> **Source**: opencode `packages/core/src/tool/{materialization,scope,provenance}.ts`

## Purpose

扩展现有 `synthia-tool::scoped_registry::ScopedToolRegistry` (618 行 LIFO + RAII)，新增 `ToolId` / `ProviderId` / `ToolVisibility` / `Materialization` / `ToolProvenance` 5 个 type，引入 `Scope.fork` 和 `whollyDisabled` filter，向后兼容现有 LIFO + RAII 语义。

## ADDED Requirements

### Requirement: ToolId + ProviderId newtype

The `synthia-tool-materialization` crate MUST expose `ToolId(Uuid)` and `ProviderId(&'static str)` as opaque newtypes with serde + Display impls.

#### Scenario: tool id creation

- **WHEN** `ToolId::new_v4()` is called
- **THEN** the system MUST return `ToolId(Uuid::new_v4())`
- **AND** MUST be `Display`-able as the canonical hex format

#### Scenario: provider id is static

- **WHEN** `ProviderId::new("builtin.apply_patch")` is called
- **THEN** the system MUST intern the string as `&'static str` via `once_cell` lazy evaluation
- **AND** MUST reject empty strings at compile time (`const_assert`)

### Requirement: Materialization with identity field

Every call to `ScopedToolRegistry::materialize()` MUST return a `Materialization { id, provider_id, visibility, wholly_disabled, provenance, scope_fork }`.

#### Scenario: materialize returns identity

- **WHEN** `registry.materialize(tool_spec, provider_id::BUILTIN)` is called
- **THEN** the returned `Materialization` MUST include `id: ToolId` allocated uniquely for this call
- **AND** MUST include `wholly_disabled: false` by default
- **AND** MUST include `provenance: ToolProvenance::Builtin`

#### Scenario: existing LIFO + RAII preserved

- **WHEN** the new identity field is added
- **THEN** all existing tests in `synthia-tool::tests::scoped_registry_*` MUST continue to pass without modification

### Requirement: whollyDisabled filter

`ScopedToolRegistry::resolve()` MUST skip tools whose most recent `Materialization` has `wholly_disabled == true`.

#### Scenario: wholly disabled tool blocked

- **WHEN** a tool is materialized with `wholly_disabled: true`
- **THEN** `resolve()` MUST return `None` for that tool id
- **AND** MUST log `tool_wholly_disabled` with the tool id

#### Scenario: re-enable via materialize

- **WHEN** a disabled tool is re-materialized with `wholly_disabled: false`
- **THEN** subsequent `resolve()` MUST return it again
- **AND** the older disable MUST be garbage-collected

### Requirement: ToolProvenance enum

The `Materialization` MUST carry a `ToolProvenance` from `{ Builtin, Plugin { extension_id }, Ephemeral { source_id } }`.

#### Scenario: builtin provenance

- **WHEN** a builtin tool is registered via `synthia-tool::register_builtin(...)`
- **THEN** its `Materialization::provenance` MUST be `Builtin`

#### Scenario: plugin provenance

- **WHEN** an extension registers a tool via `synthia-extension-v2::register(...)`
- **THEN** its `Materialization::provenance` MUST be `Plugin { extension_id: <id> }`

### Requirement: Scope.fork + tool_id projection

The `ScopedToolRegistry` MUST support `scope.fork(name) -> Arc<Scope>` and MUST project `tool_id` into `synthia_session::OpRun`.

#### Scenario: fork shares parent

- **WHEN** `scope.fork("subagent_42")` is called
- **THEN** the returned `Arc<Scope>` MUST retain a `Weak` reference to the parent
- **AND** MUST be collectible when the parent drops

#### Scenario: tool_id on session

- **WHEN** an `OpRun` is recorded
- **THEN** the record MUST include `tool_id: ToolId` (NOT just provider id)
- **AND** MUST be queryable for materialization audit
