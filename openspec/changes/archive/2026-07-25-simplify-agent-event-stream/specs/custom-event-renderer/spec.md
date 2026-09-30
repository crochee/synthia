## MODIFIED Requirements

### Requirement: AgentEvent custom event variant

The `AgentEvent` enum MUST expose custom events via `AgentEvent::Hook(HookEvent::Custom { kind: String, data: serde_json::Value })`.

#### Scenario: custom emission via Hook::Custom
- **WHEN** an extension emits via `EventBus::emit(AgentEvent::Hook(HookEvent::Custom { kind: "subagent.lane.created", data: json!(...) }))`
- **THEN** the system MUST serialize the envelope with the custom kind preserved
- **AND** MUST NOT reject unknown kind strings

#### Scenario: Custom event projects through the renderer registry
- **WHEN** the system starts without an explicit renderer registration for a kind
- **THEN** the builtin `JsonEventRenderer` MUST match on the wildcard kind
- **AND** MUST serialize the Custom variant as `Part::data({ kind: <kind>, ...data })` on the wire

---

## REMOVED Requirements

### Requirement: AgentEvent::Custom top-level variant

**Reason**: The legacy `AgentEvent::Custom { event_type: String, data: serde_json::Value }` top-level variant is folded into `AgentEvent::Hook(HookEvent::Custom)` so that all "external injection" events share a single top-level channel. The variant is preserved semantically (kind + data) but lives under `Hook` to keep the five-variant top-level structure stable.

**Migration**: Producers MUST emit `AgentEvent::Hook(HookEvent::Custom { kind, data })` instead of `AgentEvent::Custom { event_type, data }` (note `event_type` is renamed to `kind`). Consumers MUST match `Hook(HookEvent::Custom { kind, .. })` and read the discriminator from `kind` rather than `event_type`.