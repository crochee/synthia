# Capability: extension-system

> **Status**: Proposed (change #1: 架构基础设施)
> **Source**: opencode `packages/core/src/extension.ts` + codex `codex-rs/core/src/hooks/extension.rs`

## Purpose

将现有 `synthia-extension` (1 行 stub) 替换为完整 typed extension system，支持 19 typed events、Extension manifest、sandbox infrastructure、ExtensionRegistry。与现有双 hook 系统（AgentHook + HookRunner）并行 3 月。

## ADDED Requirements

### Requirement: Extension trait with 19 typed events

The `synthia-extension-v2` system MUST expose an `Extension` trait operating on 19 typed event payloads.

#### Scenario: extension implements typed hooks

- **WHEN** a consumer implements `Extension` for their struct
- **THEN** the implementation MUST declare which of the 19 typed events it subscribes to
- **AND** MUST receive strongly-typed payloads (no `serde_json::Value` for events the user opted in)
- **AND** MUST NOT require runtime reflection

#### Scenario: 19 events coverage

- **WHEN** the system's `events!()` macro is expanded
- **THEN** the resulting enum MUST include exactly: `SessionStart`, `SessionEnd`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `PreResponse`, `PostResponse`, `PreCompact`, `PostCompact`, `PreMessageDrop` (Synthia 独有), `PreSteering`, `PostSteering`, `PreSubagentSpawn`, `PostSubagentSpawn`, `PreDefinitionDrift`, `PostDefinitionDrift`, `PreMCPRoute`, `PostMCPRoute`, `PreOAuthFlow`
- **AND each event MUST carry a distinct typed payload struct**

### Requirement: ExtensionManifest for declarative registration

The system MUST support a declarative `ExtensionManifest` parsed from TOML or built programmatically.

#### Scenario: manifest parse

- **WHEN** a manifest is loaded from `extensions.d/<id>.toml`
- **THEN** the parser MUST validate the `[capabilities]` section against the typed contract
- **AND** MUST reject unknown capability names with a typed `ExtensionManifestError`
- **AND** MUST log a warning when the manifest declares more capabilities than the implementation provides

#### Scenario: programmatic manifest

- **WHEN** a consumer calls `ExtensionManifest::builder().name(...).capability(...)`
- **THEN** the builder MUST enforce non-empty `name` + at least one capability
- **AND** MUST return `ExtensionManifestError::EmptyName` if name is empty

### Requirement: typed capability-scoped sandbox

The system MUST execute extension callbacks inside a typed capability-scoped execution boundary.

#### Scenario: capability boundary enforcement

- **WHEN** an extension's `PreToolUse` callback is invoked
- **AND** the tool requires `network` capability
- **THEN** the executor MUST check whether the extension's `ExtensionManifest` declares `network`
- **AND** MUST refuse to invoke the callback if the capability is missing
- **AND** MUST log `extension_capability_violation` with extension id + required capability

#### Scenario: capability violation metrics

- **WHEN** a callback is refused due to missing capability
- **THEN** a `prometheus` counter `extension_capability_violation_total{event,capability}` MUST increment
- **AND** the callback MUST receive `HookOutcome::Deny { reason }` (see hook-system-unification)

### Requirement: ExtensionRegistry with double-registration

The system MUST provide an `ExtensionRegistry` and MUST coexist with `ServiceRegistry` via double-registration.

#### Scenario: extension registered to both registries

- **WHEN** `ExtensionRegistry::register(ext)` is called
- **THEN** the registry MUST also call `ServiceRegistry::register_as_extension(ext.id(), ext)` automatically
- **AND** MUST reject duplicate ids with `ExtensionRegistryError::DuplicateId`

#### Scenario: extension deregistered atomically

- **WHEN** `ExtensionRegistry::deregister(id)` is called
- **THEN** the registry MUST atomically remove from both `ExtensionRegistry` and `ServiceRegistry`
- **AND** MUST NOT remove extensions registered via `ServiceRegistry::register_as_extension` without an explicit `deregister`

### Requirement: backward compatibility with 1-line stub

The existing `synthia-extension::lib.rs` 1-line stub MUST continue to compile until 6 月 deprecation milestone.

#### Scenario: stub still works

- **WHEN** a consumer depends on `synthia-extension` crate version 0.3
- **THEN** the existing API MUST remain available
- **AND a deprecation warning MUST be emitted at compile time**
