# Capability: custom-event-renderer

> **Status**: Proposed (change #1: 架构基础设施)
> **Source**: pi-mono `extensions/types.ts` + `messages.ts`

## Purpose

在 `synthia-agent::events::AgentEvent` 的 `HookEvent::Custom` variant 上提供 `EventRenderer` registry + builtin JSON renderer，将 Custom event 投影到 `AgentMessage`。

## Requirements

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

### Requirement: EventRenderer registry

The `synthia-extension-v2` crate MUST expose an `EventRendererRegistry` keyed by `event_type` string.

#### Scenario: builtin json renderer

- **WHEN** the system starts without an explicit renderer registration
- **THEN** the registry MUST contain a builtin `JsonEventRenderer` for `*` (wildcard)
- **AND** MUST serialize the Custom variant as `{"type": event_type, "data": data}`

#### Scenario: custom renderer registration

- **WHEN** `renderer_registry.register("subagent.lane.created", MyRenderer::new())` is called
- **THEN** subsequent Custom events with that `event_type` MUST be rendered by `MyRenderer`
- **AND** MUST shadow the builtin wildcard renderer

### Requirement: projection to AgentMessage

The system MUST project Custom events to `synthia-protocol::AgentMessage` for downstream consumers.

#### Scenario: default projection

- **WHEN** a Custom event reaches the protocol layer
- **AND** no renderer is registered for `event_type`
- **THEN** the projection MUST use the builtin JSON renderer
- **AND** MUST append a `Metadata { kind: "custom_event" }` block to the message

#### Scenario: missing renderer fallback

- **WHEN** a renderer for `event_type` throws during rendering
- **THEN** the system MUST log `event_renderer_failed` with the event id
- **AND** MUST fall back to the builtin JSON renderer
- **AND** MUST NOT drop the event silently
