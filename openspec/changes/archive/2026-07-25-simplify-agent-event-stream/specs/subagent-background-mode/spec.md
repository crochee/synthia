## MODIFIED Requirements

### Requirement: Background task results SHALL be injected as structured XML

Completed background tasks SHALL be injected into the parent context as a message containing structured XML wrapping the task id, state, summary, and task result. This requirement does not depend on the legacy AgentEvent variant names; the wire shape remains unchanged.

#### Scenario: Background task result injection
- **WHEN** a background task completes
- **THEN** its result MUST be injected into the parent context as a structured XML block

---

## REMOVED Requirements

### Requirement: Subagent* lifecycle events as discrete AgentEvent variants

**Reason**: The legacy `AgentEvent::SubagentSpawnBegin`, `SubagentSpawnEnd`, `SubagentMessage`, `SubagentComplete`, and `SubagentCompleted` variants are replaced by the unified `AgentEvent::Agent(AgentMeta, Box<AgentEvent>)` wrapper. Subagent lifecycle boundaries are expressed via nested `SystemEvent::SessionStarted | SessionEnded` events rather than dedicated top-level variants.

**Migration**: Background-mode producers MUST emit `AgentEvent::Agent(AgentMeta, Box::new(inner))` where `inner` carries the lifecycle event. Consumers MUST pattern-match `AgentEvent::Agent(meta, inner)` and inspect `inner` to determine lifecycle state.