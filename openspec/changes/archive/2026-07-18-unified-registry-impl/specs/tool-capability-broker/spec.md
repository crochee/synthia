## ADDED Requirements

### Requirement: Tool Capabilities Allow-List
Each tool SHALL declare a `ToolCapabilities` struct at registration time with boolean flags: `memory_read`, `memory_write`, `session_fork`, `permission_record`, `hook_emit`, `telemetry_record`, `skill_invoke`, `command_invoke`. Default SHALL be all-false (pure function tool).

#### Scenario: Pure function tool default capabilities
- **WHEN** a tool is registered without explicit capabilities
- **THEN** all capability flags SHALL be false — the tool cannot access any service

#### Scenario: Tool declares needed capabilities
- **WHEN** a `GrepTool` registers with `memory_read: true`
- **THEN** the `CapabilityBroker` SHALL allow memory read access and deny all other service access

---

### Requirement: CapabilityBroker Enforcement
`ToolContext` SHALL carry a `CapabilityBroker` (NOT `Arc<ServiceRegistry>`). Calling a service method whose capability flag is `false` SHALL return `ToolError::CapabilityDenied` with the denied service key.

#### Scenario: Capability denied
- **WHEN** a tool calls `broker.memory_read()` but `capabilities.memory_read == false`
- **THEN** the call SHALL return `ToolError::CapabilityDenied { service: "MemoryService", need: "memory_read" }`

#### Scenario: Capability allowed
- **WHEN** a tool calls `broker.memory_read()` and `capabilities.memory_read == true`
- **THEN** the call SHALL return the `Arc<dyn MemoryService>` handle
