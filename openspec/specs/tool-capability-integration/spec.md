# tool-capability-integration Specification

## Purpose
TBD - created by archiving change synthia-tool-orchestrator-permission. Update Purpose after archive.
## Requirements
### Requirement: ToolCapabilities field in ToolExecutionContext

`synthia-tool::ToolExecutionContext` SHALL have a `capabilities: Option<ToolCapabilities>` field.

#### Scenario: ToolExecutionContext with capabilities

WHEN a `ToolExecutionContext` is constructed with `capabilities: Some(ToolCapabilities { memory_read: true, ..default() })`
THEN `ctx.capabilities` SHALL return `Some(&ToolCapabilities)` with the specified values

#### Scenario: ToolExecutionContext without capabilities

WHEN a `ToolExecutionContext` is constructed with `capabilities: None`
THEN `ctx.capabilities` SHALL return `None`
AND the tool execution SHALL proceed without capability checks

### Requirement: ToolAdapter SHALL populate capabilities from ToolContext

`ToolAdapter` SHALL populate the `capabilities` field on `ToolExecutionContext` from `synthia-core::ToolContext` when the `unified-registry` feature is enabled.

WHEN `ToolAdapter::execute()` is called with the `unified-registry` feature enabled
AND the source `synthia-core::ToolContext` has `capabilities` populated
THEN `ToolAdapter` SHALL copy the capabilities into the constructed `ToolExecutionContext`

#### Scenario: Unified-registry feature enabled with capabilities

WHEN `ToolAdapter::execute()` is called with `unified-registry` feature and the tool context has `capabilities: ToolCapabilities { command_invoke: true, .. }`
THEN the `ToolExecutionContext` passed to the tool's `call()` method SHALL have `capabilities: Some(ToolCapabilities { command_invoke: true, .. })`

#### Scenario: Unified-registry feature disabled

WHEN `ToolAdapter::execute()` is called without the `unified-registry` feature
THEN the `ToolExecutionContext` SHALL have `capabilities: None`

### Requirement: CapabilityBroker gate in orchestrator

`DefaultToolOrchestrator::execute()` SHALL check `CapabilityBroker::allowed()` before executing tools that declare capabilities.

#### Scenario: Capability denied

WHEN `DefaultToolOrchestrator::execute()` is called for a tool requiring `command_invoke` capability
AND `CapabilityBroker::allowed("command_invoke")` returns `false`
THEN the orchestrator SHALL return a `ToolCallResult` with `is_error: true` and error message indicating capability denied

#### Scenario: Capability allowed

WHEN `CapabilityBroker::allowed("command_invoke")` returns `true`
THEN the orchestrator SHALL proceed with execution

