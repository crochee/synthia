# Spec: provenance-capability-permission

## ADDED Requirements

### Requirement: Provenance-based permission floor

The system SHALL enforce a minimum permission level based on `ToolProvenance`:

| Provenance | Minimum Level |
|------------|---------------|
| `Builtin` | `AutoApprove` |
| `Plugin { extension_id }` | `RequireConfirm` |
| `Ephemeral { source_id }` | `RequireExplicit` |

The provenance floor SHALL prevent permission level downgrade below the minimum.

#### Scenario: Builtin tool can be AutoApproved

WHEN a tool has `Provenance::Builtin` and `PermissionChecker` returns `AutoApprove`
THEN the effective permission SHALL be `AutoApprove`

#### Scenario: Plugin tool cannot be AutoApproved

WHEN a tool has `Provenance::Plugin { extension_id: "ext-1" }` and `PermissionChecker` returns `AutoApprove`
THEN the effective permission SHALL be upgraded to `RequireConfirm` (the provenance floor)

#### Scenario: Ephemeral tool cannot be AutoApproved or RequireConfirm

WHEN a tool has `Provenance::Ephemeral { source_id: "tmp-1" }` and `PermissionChecker` returns `RequireConfirm`
THEN the effective permission SHALL be upgraded to `RequireExplicit` (the provenance floor)

### Requirement: Capability-based permission upgrade within provenance floor

Within the provenance floor, `CapabilityBroker::allowed()` SHALL be consulted. If a capability is not allowed, the permission level SHALL be upgraded (more restrictive).

#### Scenario: Plugin with command_invoke capability denied

WHEN a Plugin tool requires `command_invoke` capability AND `CapabilityBroker::allowed("command_invoke")` returns `false`
THEN the effective permission SHALL be upgraded to `Deny` regardless of the provenance floor

#### Scenario: Plugin with memory_read capability allowed

WHEN a Plugin tool requires `memory_read` capability AND `CapabilityBroker::allowed("memory_read")` returns `true`
THEN the effective permission SHALL remain at the provenance floor (`RequireConfirm`)

### Requirement: Provenance-Capability evaluation in orchestrator

`DefaultToolOrchestrator::execute()` SHALL evaluate provenance floor + capability upgrade before the approval phase.

#### Scenario: Provenance evaluation before approval

WHEN `execute()` is called
THEN the orchestrator SHALL first resolve the tool's `ToolProvenance`
AND compute the provenance floor
AND check capabilities via `CapabilityBroker`
AND compute the effective permission
AND then proceed to the approval phase with the effective permission
