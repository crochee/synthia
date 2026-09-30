# Capability: custom-event-renderer

> **Status**: Proposed (change #1: 架构基础设施)
> **Source**: pi-mono `extensions/types.ts` + `messages.ts`

## Purpose

在 `synthia-agent::events::AgentEvent` 加 `Custom` variant，并在 `synthia-extension-hook` 下提供 `EventRenderer` registry + builtin JSON renderer，将 Custom event 投影到 `AgentMessage`。

## ADDED Requirements

### Requirement: AgentEvent::Custom variant

The existing `AgentEvent` enum (28-variant) MUST gain a `Custom { event_type: String, data: serde_json::Value }` variant.

#### Scenario: custom emission

- **WHEN** an extension emits via `EventBus::emit(AgentEvent::Custom { event_type: "subagent.lane.created", data: json!(...) })`
- **THEN** the system MUST serialize the envelope with the custom variant tag preserved
- **AND** MUST NOT reject unknown event_type strings

#### Scenario: 28 existing variants unchanged

- **WHEN** the new variant is added
- **THEN** all 28 existing variants MUST remain in their current positions (so JSON serialization order is stable)
- **AND** `serde` derive order MUST be preserved

### Requirement: EventRenderer registry

The `synthia-extension-hook` crate MUST expose an `EventRendererRegistry` keyed by `event_type` string.

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
