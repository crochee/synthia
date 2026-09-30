# output-ui

## ADDED Requirements

### Requirement: Output/UI scope SHALL expose 4 extension points

The Output/UI scope SHALL expose: `output.format`, `output.metadata.inject`, `ui.dialog.{select, confirm, input, notify}`, `ui.render.component`.

#### Scenario: output.format intercepts user-facing text
- **WHEN** the agent loop emits text to the user (e.g., a tool output snippet, a final answer)
- **THEN** `output.format` SHALL be fired with `OutputFormatInput { content: String, mime: MimeType, audience: Audience }` by mutable reference
- **AND** the extension MAY rewrite `content` (e.g., collapse verbose JSON, strip ANSI codes)
- **AND** the modified content SHALL be the one displayed to the user
- **AND** P1 prefix consistency: transformations MUST be deterministic across calls (output formatting is part of the rendered user view, not the LLM context)

#### Scenario: output.metadata.inject adds structured fields
- **WHEN** `output.metadata.inject` fires for a given output
- **THEN** the extension SHALL return `MetadataPatch { fields: BTreeMap<String, MetadataValue> }`
- **AND** the orchestrator SHALL merge the patch into the output's metadata
- **AND** conflicts SHALL resolve in registration order (first-registered extension wins)

#### Scenario: ui.dialog.notify is non-blocking
- **WHEN** an extension calls `ui.dialog.notify` with `NotifyRequest { message: String, level: NotificationLevel }`
- **THEN** a notification SHALL appear in the host (TUI / RPC / Server)
- **AND** the notification SHALL NOT block the agent loop
- **AND** the orchestrator SHALL be able to map `NotificationLevel::{Info, Warning, Error}` to host-specific UI

#### Scenario: ui.dialog.confirm blocks for user response
- **WHEN** an extension calls `ui.dialog.confirm` with `ConfirmRequest { prompt: String, default: bool, timeout_ms: Option<u32> }`
- **THEN** the agent loop SHALL block on the user's response
- **AND** the extension SHALL receive `bool` (the user's choice) when the user responds
- **AND** if `timeout_ms` is set and the timeout elapses, the extension SHALL receive `default`

#### Scenario: ui.render.component produces a typed widget
- **WHEN** `ui.render.component` fires with `RenderRequest { kind: ComponentKind, props: serde_json::Value }`
- **THEN** the extension SHALL return `RenderOutput { component: ComponentKind, rendered: serde_json::Value }`
- **AND** the host SHALL render the typed widget (TUI: text/diff/table; RPC: JSON; Server: HTML/SSR)
- **AND** unsupported `ComponentKind` values SHALL fall back to plain text

### Requirement: Output/UI scope SHALL respect host capability mapping

The Output/UI scope SHALL map extension-level UI primitives to
host-specific renderers (TUI / RPC / Server). When a host cannot
render a given primitive, the orchestrator SHALL fall back to a
safe default rather than fail.

#### Scenario: TUI host renders text and diff
- **WHEN** the TUI host receives a `ui.render.component` request with `ComponentKind::Text` or `ComponentKind::Diff`
- **THEN** the TUI SHALL render the component natively
- **AND** unsupported kinds (e.g., `ComponentKind::Chart`) SHALL fall back to `String` rendering

#### Scenario: RPC host renders JSON-only
- **WHEN** the RPC host receives a `ui.render.component` request
- **THEN** the host SHALL serialize the rendered component to JSON
- **AND** typed widget metadata SHALL be preserved in a `component_kind` field

#### Scenario: Server host renders HTML/SSR
- **WHEN** the Server host receives a `ui.render.component` request
- **THEN** the host SHALL render the component to HTML
- **AND** client-side hydration data SHALL be included in a `data-component` attribute

### Requirement: Output/UI used-by matrix SHALL be maintained per point

The Output/UI scope SHALL maintain a "Used by / Reserved for" matrix for every extension point. The matrix SHALL be the single source of truth documenting which points are exercised by current code vs. reserved for future use.

| Extension point | Used by | Reserved for |
|---|---|---|
| `output.format` | — (reserved) | JSON collapse, ANSI stripping, language detection |
| `output.metadata.inject` | — (reserved) | Tracing IDs, timestamps, source attribution |
| `ui.dialog.{select, confirm, input, notify}` | — (reserved) | User prompts, permission re-prompts, progress notifications |
| `ui.render.component` | — (reserved) | Rich rendering (diff, table, chart) when the host supports it |

#### Scenario: used-by matrix SHALL be the source of truth for current consumers
- **WHEN** a developer checks which Output/UI extension points are exercised by current code
- **THEN** the "Used by" column SHALL accurately list every internal call site
- **AND** the "Reserved for" column SHALL list at least one concrete future use case per point
- **AND** any discrepancy SHALL be reported as a documentation bug
