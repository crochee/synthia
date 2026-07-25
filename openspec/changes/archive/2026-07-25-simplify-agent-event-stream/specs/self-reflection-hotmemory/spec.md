## REMOVED Requirements

### Requirement: Self-reflection AgentEvent variant

**Reason**: The legacy `AgentEvent::SelfReflection { ... }` variant is removed. Self-reflection is now a regular tool invocation — the LLM emits `AgentEvent::Model(ContentPart::ToolUse(ToolUse { name: "self_reflection", input }))` and receives `AgentEvent::Model(ContentPart::ToolResult(...))`. This unifies the reflection flow with every other tool and removes a one-off top-level variant that did not generalise.

**Migration**: Producers MUST emit `ToolUse` with `name == "self_reflection"` (matching the existing reflection tool registration) instead of `AgentEvent::SelfReflection`. Consumers observing reflection events MUST match the tool-name field on `ContentPart::ToolUse` rather than a dedicated variant. The HotMemory key pattern and reflection data shape documented in the remaining requirements are unchanged.

---

## MODIFIED Requirements

### Requirement: Self-reflection shall be executed after main loop completion

Self-reflection SHALL be triggered after the main loop ends with a successful completion (end_reason = Completed and iteration > 0).

#### Scenario: Self-reflection trigger condition
- **WHEN** Main loop exits with end_reason = Completed
- **AND** iteration > 0
- **THEN** Self-reflection SHALL be executed

#### Scenario: Self-reflection skipped on early exit
- **WHEN** Main loop exits with Cancelled, LoopDetected, or Error
- **THEN** Self-reflection SHALL be skipped
- **AND** No reflection artifact SHALL be generated