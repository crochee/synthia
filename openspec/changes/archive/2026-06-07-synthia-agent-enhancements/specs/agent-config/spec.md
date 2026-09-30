## ADDED Requirements

### Requirement: Extended AgentRunConfig Fields
The `AgentRunConfig` SHALL support optional fields: `agent_control: Option<Arc<AgentControl>>` for multi-agent control plane integration, and `fork_policy: ForkPolicy` for sub-agent spawning behavior.

### Requirement: PermissionRules Field
The `AgentDefinition` SHALL include `permission_rules: Vec<PermissionRule>` field. This field SHALL be populated from file-based Agent definitions or programmatically created Agents.

### Requirement: PermissionDefault Field
The `AgentDefinition` SHALL include `permission_default: Option<PermissionAction>` field. When `None`, the MergedPolicy SHALL use the Default layer rules. When `Some(action)`, this SHALL override the default behavior.

### Requirement: Tools and DeniedTools Fields
The `AgentDefinition` SHALL include `tools: Option<Vec<ToolId>>` for allowlist and `denied_tools: Option<Vec<ToolId>>` for denylist. The `allowed_tools` SHALL filter at ToolRegistry level before MergedPolicy. The `denied_tools` SHALL append `forced: true` Deny rules to MergedPolicy Agent layer.

### Requirement: Extends Field
The `AgentDefinition` SHALL include `extends: Option<AgentId>` field for inheritance. The system SHALL resolve the extends chain at load time, with maximum depth of 4 levels. Circular extends SHALL be detected and rejected.

### Requirement: Mode Field Mapping
The `AgentDefinition.mode` field SHALL map to `ExecutorKindConfig` (Build/Plan/General). The mode affects which executor is used for the agent's main loop.

---

## MODIFIED Requirements

None — this is a new capability.

---

## REMOVED Requirements

None — this is a new capability.

---

## RENAMED Requirements

None.