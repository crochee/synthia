## ADDED Requirements

### Requirement: ForkPolicy Message Strategies
The system SHALL define `ForkPolicy` with5 variants: `InheritAll` (full history), `LastNTurns(n)`, `SinceStep(step)`, `ByTag(tag)`, `Empty`, `SystemOnly`. The default SHALL be `SystemOnly`. ForkPolicy SHALL NOT inherit parent messages — it only controls message history transmission.

### Requirement: ForkPermissionPolicy Permission Strategies
The system SHALL define `ForkPermissionPolicy` with 4 variants: `InheritAll` (inherit all three layers), `InheritAsUser` (parent Agent layer becomes sub-agent User layer), `InheritAsAgent` (parent Agent layer becomes sub-agent Agent layer), `Empty` (sub-agent only sees defaults). The default SHALL be `InheritAsUser`.

### Requirement: Combined Fork Defaults
When LLM calls `AgentTool` without explicit fork parameters, the system SHALL use default combination `ForkPolicy::SystemOnly + ForkPermissionPolicy::InheritAsUser`. CLI/config.yaml MAY override these defaults globally.

### Requirement: ForkPolicy Message Filtering
When forking with history, the system SHALL filter rollout items using `keep_forked_rollout_item` logic: System/developer/user messages are always copied; assistant messages are copied only if `phase == FinalAnswer`; reasoning items, shell calls, function calls are stripped. The child agent receives a clean context.

### Requirement: Definition Drift Detection
When a sub-agent completes, if the parent's definition has changed (different content_hash), the system SHALL emit a `definition_drift` warning as telemetry only. This SHALL NOT cancel or block the sub-agent completion.

---

## MODIFIED Requirements

None — this is a new capability.

---

## REMOVED Requirements

None — this is a new capability.

---

## RENAMED Requirements

None.