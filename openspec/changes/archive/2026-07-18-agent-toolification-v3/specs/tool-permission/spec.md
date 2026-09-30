## ADDED Requirements

### Requirement: ToolPermission Trait

The `synthia-tools` crate MUST export a `ToolPermission` trait with method `fn check(&self, ctx: &PermissionContext) -> PermissionDecision`. `PermissionDecision` MUST be an enum with at least `Allow`, `Deny(reason: String)`, and `Ask` variants. A `PermissionAlwaysAllow` implementation MUST be provided as the default.

#### Scenario: PermissionDecision Variants

- **WHEN** a developer inspects `PermissionDecision`
- **THEN** it SHALL have `Allow`, `Deny(String)`, and `Ask` variants, each carrying structured information for logging

#### Scenario: Default Allow

- **WHEN** `PermissionAlwaysAllow::check()` is called with any context
- **THEN** it SHALL return `PermissionDecision::Allow`

### Requirement: Permission-Aware Tool Execution

The `ToolExecution::execute()` method MUST consult the configured `ToolPermission` before performing the side effect. If permission returns `Deny`, the tool MUST return `Err(ToolError::PermissionDenied)` without performing the side effect.

#### Scenario: Deny Path

- **WHEN** `ToolPermission::check()` returns `Deny("reason")`
- **THEN** `ToolExecution::execute()` MUST return `Err` and MUST NOT mutate any external state

#### Scenario: Ask Path Resolution

- **WHEN** `ToolPermission::check()` returns `Ask` and the user approves
- **THEN** the tool MUST proceed with execution as if `Allow` was returned

### Requirement: PermissionContext Type

The `PermissionContext` type MUST carry at minimum: `tool_name: String`, `arguments: serde_json::Value`, `agent_run_id: Uuid`, and `user_id: Option<String>`. The context MUST be `Clone + Send + Sync + 'static`.

#### Scenario: Context Availability

- **WHEN** a permission policy implementation inspects `PermissionContext`
- **THEN** it SHALL have access to all four fields above for policy decision making