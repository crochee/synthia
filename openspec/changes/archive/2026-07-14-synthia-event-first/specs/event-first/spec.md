# Delta Spec: synthia-event-first (Change 2 of v3 architecture)

> **Archive Note (2026-07-14):** Delta spec synthesized at archive time. The originally-proposed `specs/` was empty; this document captures the **verified** capability surface that landed via v3 commits `3e5940c..6288a5b` and the production-grade-agent-architecture lineage (notably `74dd673 feat(extension): 43 extension points across 8 scopes + 64-point matrix`).

## Purpose

Re-orient Synthia around a typed event bus as the source of truth. Replace hardcoded branches in `main_loop.rs`, `doom_loop_handler.rs`, and the permission `MergedPolicy` machinery with 43+ extension points across 8 scopes, each reachable through the `ExtensionRegistry` and emitting an `extension.hook` OTel span per `fire_*` call (P9 hard constraint).

## ADDED Requirements

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

## Out of Scope (deferred to other Changes)

- Submission/EventMsg wire protocol — Change 3
- JSONL append-only session tree — Change 3
- Provider hot-swap with `source_id` isolation — Change 3 R7
- 9-abstractions toolification (external_hook_tool + plugin CLI as Tool) — Change 3 R8
- Compile-time extension loading (jiti-style) — explicitly rejected
- WASM tool provider — explicitly rejected

## MODIFIED Requirements

### `synthia-agent::DoomLoopHandler` → `DefaultDoomLoopExtension`

The hardcoded `doom_loop_handler.rs` (86 LOC) SHALL be deleted. DoomLoop detection SHALL be implemented as a `DefaultDoomLoopExtension` that subscribes to `ToolCall` events and fires `DoomLoopDetected` when 3 consecutive same-fingerprint tool calls occur within 30s.

### `synthia-permission::ApprovalService` gains `PermissionFuture::from_event`

A new async path SHALL be added: `PermissionFuture::from_event(req, reply_tx) -> PermissionFuture`. The sync `ApprovalService::check(...)` SHALL be deprecated for 1 minor cycle.

## Reference

- Parent design: `docs/superpowers/specs/2026-07-12-synthia-v3-tool-first-architecture-design.md`
- pi-mono pattern: 27-event union + extension-first design
- opencode pattern: `permission.asked`/`replied` bus
- codex pattern: `AskForApproval` + sandbox-denial escalation
- Implementation commits: `74dd673`, `ef2eaac`, `ec74cff`, `586f7ae`
