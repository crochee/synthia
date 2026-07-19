# event-first Specification

## Purpose
TBD - created by archiving change synthia-event-first. Update Purpose after archive.
## Requirements
### Requirement: `ExtensionRegistry` with wildcard matching and OTel span enforcement

The `ExtensionRegistry` SHALL provide typed `register(id, handler)` and `emit(event) -> Action<T>` methods. Every `emit` SHALL open an `extension.hook` OTel span (P9). Wildcard subscription via `*` SHALL be supported for the v3 Phase 3 reuse path.

#### Scenario: Wildcard subscription receives all events

- **WHEN** an extension is registered with id `*`
- **THEN** every emitted `ExtensionEvent` SHALL be dispatched to that handler in registration order
- **AND** an `extension.hook` OTel span SHALL be opened for each dispatch

#### Scenario: VERIFIED

- **GIVEN** v3 commit `74dd673 feat(extension): 43 extension points across 8 scopes + 64-point matrix`
- **WHEN** running `cargo test -p synthia-agent --test extension_matrix`
- **THEN** all 64 points are reachable and emit the expected OTel span

### Requirement: 8-scope extension point matrix

The following 8 scopes SHALL each expose a typed `*ExtensionRegistry` with `register_*` / `fire_*` methods, gated by `PermissionExtensibilityGuard` where mutation is allowed:

| Scope | Crate path | Count |
|-------|-----------|-------|
| `tool` | `crates/synthia-agent/src/tools/dynamic_provider/extension_points/tool.rs` | 7 |
| `agent_loop` | `.../extension_points/agent_loop.rs` | Phase 3 |
| `llm` | `.../extension_points/llm.rs` | Phase 3 |
| `context` | `.../extension_points/context.rs` | Phase 3 |
| `permission` | `.../extension_points/permission.rs` | 5 |
| `provider` | `.../extension_points/provider.rs` | 4 |
| `event_bus` | `.../extension_points/event_bus.rs` | 4 |
| `plugin_lifecycle` | `.../extension_points/plugin_lifecycle.rs` | 6 |
| `session_tree` | `.../extension_points/session_tree.rs` | 5 |
| `output_ui` | `.../extension_points/output_ui.rs` | 4 |

Total: **43+ extension points** across 8 scopes (R2-R4 absorbed).

#### Scenario: VERIFIED

- **GIVEN** v3 commit `74dd673` lands
- **WHEN** listing extension points in `crates/synthia-agent/src/tools/dynamic_provider/extension_points/`
- **THEN** all 8 scope files exist with typed registries

### Requirement: Permission fail-closed default (P6)

If no listener fires within 50ms of a `PermissionAsk` event, the fallback policy SHALL be `Ask` (not `Allow`). The `PermissionExtensibilityGuard` SHALL downgrade any extension returning `Allow` while the base policy is `Deny`/`Ask` to `Ask`.

#### Scenario: No-listener path falls back to Ask

- **WHEN** a `PermissionAsk` event is emitted with no registered listeners
- **AND** the 50ms grace window elapses
- **THEN** the resolved decision SHALL be `Ask` (P6 fail-closed)

#### Scenario: Weakening attempt is downgraded

- **GIVEN** a base policy of `Deny`
- **WHEN** an extension returns `Allow` via `Action<PermissionDecision>`
- **THEN** `PermissionExtensibilityGuard::downgrade_weaken_to_ask` SHALL rewrite it to `Ask`
- **AND** a `permission.weakening_attempt` OTel event SHALL be logged

