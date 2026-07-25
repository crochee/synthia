# tool-id-audit-trail Specification

## Purpose
TBD - created by archiving change synthia-tool-orchestrator-permission. Update Purpose after archive.
## Requirements
### Requirement: ToolId on ToolCallRequest

`ToolCallRequest` SHALL have a `tool_id: Option<ToolId>` field.

#### Scenario: ToolCallRequest with ToolId

WHEN a `ToolCallRequest` is constructed with `tool_id: Some(ToolId::new())`
THEN `request.tool_id` SHALL return `Some(&ToolId)`

#### Scenario: ToolCallRequest without ToolId

WHEN a `ToolCallRequest` is constructed with `tool_id: None` (e.g., programmatic or test calls)
THEN `request.tool_id` SHALL return `None`

### Requirement: ToolId on ToolCallResult

`ToolCallResult` SHALL have a `tool_id: Option<ToolId>` field that echoes the request's `tool_id`.

#### Scenario: ToolCallResult echoes request ToolId

WHEN `DefaultToolOrchestrator::execute()` is called with a `ToolCallRequest` that has `tool_id: Some(id)`
THEN the returned `ToolCallResult` SHALL have `tool_id: Some(id)` matching the request

#### Scenario: ToolCallResult with no ToolId

WHEN the request has `tool_id: None`
THEN the result SHALL have `tool_id: None`

### Requirement: Orchestrator SHALL populate ToolId from Materialization

The orchestrator SHALL populate `ToolId` on `ToolCallRequest` from the tool's `Materialization` when a `ToolIdResolver` is configured.

WHEN `DefaultToolOrchestrator` resolves a tool via `ToolResolver` and the resolved tool has a `Materialization` with a `ToolId`
THEN the orchestrator SHALL set `request.tool_id = Some(materialization.id)` before execution

#### Scenario: Resolved tool has Materialization

WHEN `HashMapResolver` returns a tool with `Materialization { id: ToolId(uuid), .. }`
THEN the orchestrator SHALL populate `tool_id: Some(ToolId(uuid))` on the request

#### Scenario: Resolved tool has no Materialization

WHEN `HashMapResolver` returns a tool without Materialization data
THEN `tool_id` SHALL remain `None`

### Requirement: ToolOrchestratorEvent carries ToolId

`ToolOrchestratorEvent` variants SHALL include `tool_id: Option<ToolId>` for event correlation.

#### Scenario: ToolExecuted event has ToolId

WHEN a tool execution completes
THEN the `ToolOrchestratorEvent::ToolExecuted` event SHALL carry `tool_id: Some(id)` matching the request

