# plugin-lifecycle

## ADDED Requirements

### Requirement: Plugin Lifecycle scope SHALL expose 6 extension points

The Plugin Lifecycle scope SHALL expose: `extension.load`, `extension.bind`, `extension.invalidate`, `extension.unload`, `extension.hot_swap`, `extension.dual_form`.

#### Scenario: extension.load transitions Loading state
- **WHEN** `extension.load` is fired
- **THEN** the `ExtensionContext` SHALL be in `Loading` state
- **AND** the extension MAY register tools via the `ExtensionRuntime` API
- **AND** pending registrations SHALL be queued

#### Scenario: extension.bind transitions to Active
- **WHEN** `extension.bind` is fired
- **THEN** the `ExtensionContext` SHALL be transitioned from `Loading` to `Active`
- **AND** the pending registration queue SHALL be flushed into the runtime
- **AND** the `bind_core` OTel span SHALL be emitted (point, scope="plugin_lifecycle", session_id, provider_count)

#### Scenario: extension.invalidate transitions to Stale
- **WHEN** `extension.invalidate` is fired
- **THEN** the `ExtensionContext` SHALL be transitioned from `Active` to `Stale`
- **AND** the last active runtime SHALL be retained for diagnostics
- **AND** subsequent `register_tool` calls SHALL fail with `StaleContextError`
- **AND** the `invalidate` OTel span SHALL be emitted (point, scope="plugin_lifecycle", from_state, retained_runtime)

#### Scenario: extension.unload is post-Stale
- **WHEN** `extension.unload` is fired
- **THEN** the `ExtensionContext` SHALL be in `Stale` state
- **AND** the last active runtime SHALL be dropped (no longer retained)
- **AND** the extension SHALL be considered fully unloaded

#### Scenario: extension.hot_swap is a 3-event sequence
- **WHEN** `extension.hot_swap` is fired with `HotSwapRequest { old_extension_id, new_extension_id }`
- **THEN** the orchestrator SHALL fire `extension.load` (new), `extension.invalidate` (old), `extension.bind` (new) in order
- **AND** the old extension's `last_active` runtime SHALL be retained until `extension.unload` (new) is fired

#### Scenario: extension.dual_form is meta
- **WHEN** `extension.dual_form` is fired with `DualFormQuery { extension_id, prefer: Tool | ExtensionTool }`
- **THEN** the extension MAY return `DualFormResponse { form: Tool | ExtensionTool, reason: String }`
- **AND** the orchestrator SHALL honor the preference for the next LLM call

### Requirement: Plugin Lifecycle scope SHALL reuse `ExtensionContext` state machine

All Plugin Lifecycle extension points SHALL operate on the existing
`ExtensionContext` three-state enum (Loading/Active/Stale) defined in
`extension_context.rs` (Phase 3.1 of the archived change). No new
states SHALL be added. Hot-swap is a 3-event sequence over the
existing states.

#### Scenario: state machine integrity under hot-swap
- **WHEN** `extension.hot_swap` fires 100 times in a tight loop
- **THEN** the `ExtensionContext` SHALL never enter an invalid state
- **AND** all in-flight calls to the old extension SHALL complete (cancellation is separate)
- **AND** the `extension.hot_swap_completed` OTel event SHALL be emitted

### Requirement: Plugin Lifecycle used-by matrix SHALL be maintained per point

The Plugin Lifecycle scope SHALL maintain a "Used by / Reserved for" matrix for every extension point. The matrix SHALL be the single source of truth documenting which points are exercised by current code vs. reserved for future use.

| Extension point | Used by | Reserved for |
|---|---|---|
| `extension.load` | — (reserved) | Plugin manager (called on `extension_manager.register`) |
| `extension.bind` | — (reserved) | Plugin manager (called on session start) |
| `extension.invalidate` | — (reserved) | Plugin manager (called on `extension_manager.unregister`) |
| `extension.unload` | — (reserved) | Plugin manager (called on shutdown) |
| `extension.hot_swap` | — (reserved) | Config-reload scenarios |
| `extension.dual_form` | — (reserved) | Plugins that can be either Tool or ExtensionTool depending on context |

#### Scenario: used-by matrix SHALL be the source of truth for current consumers
- **WHEN** a developer checks which Plugin Lifecycle extension points are exercised by current code
- **THEN** the "Used by" column SHALL accurately list every internal call site
- **AND** the "Reserved for" column SHALL list at least one concrete future use case per point
- **AND** any discrepancy SHALL be reported as a documentation bug
