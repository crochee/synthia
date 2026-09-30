## ADDED Requirements

### Requirement: StepSpawn Integration
The `StreamBuilder` SHALL support a new `StepSpawn` step type that intercepts `AgentTool` calls. When LLM calls a tool that spawns a sub-agent, StepSpawn SHALL route to the multi-agent control plane via `AgentControl::spawn_agent`.

### Requirement: Subagent AgentEvent Variants
The `AgentEvent` enum SHALL include 4 new variants for sub-agent lifecycle: `SubagentSpawnBegin`, `SubagentSpawnEnd`, `SubagentMessage`, `SubagentComplete`. These events SHALL be emitted to the event stream.

### Requirement: AgentRunConfig AgentControl Field
When `AgentRunConfig.agent_control` is `Some`, the StreamBuilder SHALL integrate with the multi-agent control plane. When `None`, the StreamBuilder SHALL operate in single-agent mode without change.

### Requirement: Ask-Suspended Coordination
When an Ask triggers via `ToolAction::PendingConfirm`, the parent StreamBuilder SHALL suspend the mailbox by transitioning `MailboxDeliveryPhase` to `Suspended`. When the Ask resolves, the StreamBuilder SHALL resume with `MailboxDeliveryPhase::NextTurn`.

---

## MODIFIED Requirements

None — this is a new capability.

---

## REMOVED Requirements

None — this is a new capability.

---

## RENAMED Requirements

None.