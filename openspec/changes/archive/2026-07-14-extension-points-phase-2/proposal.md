---
slug: extension-points-phase-2
createdAt: 2026-07-12
---

# Proposal: extension-points-phase-2

## What

Implement the remaining **43 extension points across 8 scopes** (LLM, Context,
Permission, Provider, Plugin Lifecycle, Event Bus, Session Tree, Output/UI)
specified by
[`extension-point-matrix/spec.md`](file:///home/crochee/workspace/synthia/openspec/specs/extension-point-matrix/spec.md)
§"Requirements". This change completes the 64-point matrix that the
archived `tool-abstraction-and-extensibility` change started (21 of 64
points delivered in Phase 3; 43 deferred to this follow-up).

## Why

The 64-point matrix is the foundation of the project's extensibility story:

- **Plugin authors** need stable extension points to add new behaviors without
  modifying the agent core.
- **Tests** can be written against the typed contracts, providing a strong
  guarantee that plugins don't break across agent versions.
- **The 4-scope × 64-point design** (10 scopes total per
  `extension-point-matrix/spec.md`) is the agreed-upon extension surface
  agreed to during the production-grade-agent-architecture exploration.

Without the 43 remaining points, the extension story is incomplete: an
LLM plugin can only observe (not modify) chat parameters; a security
plugin can only observe (not strengthen) permission decisions; a UI
plugin can only observe (not transform) output formatting.

## How

Reuse the patterns established in Phase 3 (extension_points/agent_loop.rs
and extension_points/tool.rs):

- **One file per scope** in `crates/synthia-agent/src/tools/dynamic_provider/extension_points/`
- **Typed event structs** (no `serde_json::Value` for inputs)
- **DashMap-backed registries** (thread-safe, O(1) lookup)
- **`Action<T>` enum** (Proceed | Modify | Skip) for mutation-pattern scopes
- **Direct handlers** (returning `()`) for observe-only scopes
- **`tracing::info_span!`** for OTel with `point` / `scope` / `extension_id` attributes
- **State machine + concurrency tests** (≥2 tests per scope)

**4 implementation rounds** (each = 1 logical commit):

| Round | Scopes | Points | Why this order |
|---|---|---|---|
| 1 | Context + LLM | 15 | Most-used, P1 (prefix) interactions first |
| 2 | Permission + Provider | 9 | P6 (fail-closed) semantics before plugins use them |
| 3 | Event Bus + Plugin Lifecycle | 10 | Meta-observability layer |
| 4 | Session Tree + Output/UI | 9 + 64-point integration test | Last-mile UI + cross-scope validation |

**3 design patterns** (one per mutation mode):

1. **Mutation** (LLM, Context, Provider, Session Tree, Output/UI): `Action<T>` with full Proceed | Modify | Skip semantics
2. **Mutation-constrained** (Permission): `Action<T>` + a guard that downgrades any "weakening" attempt to `AskUser` (P6 fail-closed)
3. **Observe-only** (Event Bus, Plugin Lifecycle, ui.render.component): direct `Fn(&Event)` handlers returning `()`

## Capabilities (8 new specs)

Each of the 8 scopes gets a dedicated spec file in
`openspec/changes/extension-points-phase-2/specs/`:

1. **llm** — 8 extension points: `system_prompt.transform`, `messages.transform`, `chat.params`, `chat.headers.inject`, `tool_choice.override`, `model.select`, `cache.breakpoint.set`, `response.transform`
2. **context** — 7 points: `context.compact.trigger`, `context.compact.summarize`, `context.compact.replace`, `context.prefix.participate`, `context.observability.emit`, `context.token_budget.adjust`, `context.message_filter`
3. **permission** — 5 points: `permission.ask`, `permission.notify`, `doom_loop.detected`, `blacklist.match`, `permission.persist`
4. **provider** — 4 points: `provider.register`, `provider.unregister`, `provider.auth`, `provider.fallback`
5. **plugin-lifecycle** — 6 points: `extension.load`, `extension.bind`, `extension.invalidate`, `extension.unload`, `extension.hot_swap`, `extension.dual_form`
6. **event-bus** — 4 points: `event.subscribe`, `event.publish`, `event.aggregate`, `event.replay`
7. **session-tree** — 5 points: `session.entry.append`, `session.tree_walk`, `session.branch.create`, `session.version.migrate`, `session.compaction.preserve`
8. **output-ui** — 4 points: `output.format`, `output.metadata.inject`, `ui.dialog.select|confirm|input|notify`, `ui.render.component`

Total: **43 extension points**, **8 spec files**, **≥24 new tests** (plus a
64-point cross-scope integration test in Round 4).

## Out of scope

- **Phase 5 (PluginHookAdapter)** and **Phase 6 (Integration + E2E)** of the
  archived `tool-abstraction-and-extensibility` plan. These are separate
  changes. They will be created once this change is archived.
- **`2.2.3 ExternalHookTool`** and **`2.3.2 Plugin CLI as Tool`** — deferred
  from the archived change. Still separate.
- **New scopes beyond the 10 specified** in
  `extension-point-matrix/spec.md`. YAGNI.

## Links

- Brainstorm: [brainstorm.md](file:///home/crochee/workspace/synthia/openspec/changes/extension-points-phase-2/brainstorm.md)
- Design: [design.md](file:///home/crochee/workspace/synthia/openspec/changes/extension-points-phase-2/design.md)
- Spec: [extension-point-matrix/spec.md](file:///home/crochee/workspace/synthia/openspec/specs/extension-point-matrix/spec.md)
- Archived plan reference: [tool-abstraction-and-extensibility/plan.md §4](file:///home/crochee/workspace/synthia/openspec/changes/archive/2026-07-12-tool-abstraction-and-extensibility/plan.md)

## Tasks (overview)

- **Round 1 (Context + LLM)**: ~15 extension points, ~6-8 tests, 1 commit
- **Round 2 (Permission + Provider)**: ~9 extension points, ~4-6 tests, 1 commit
- **Round 3 (Event Bus + Plugin Lifecycle)**: ~10 extension points, ~4-6 tests, 1 commit
- **Round 4 (Session Tree + Output/UI)**: ~9 extension points, ~4-6 tests + 64-point integration test, 1 commit

Full task list in [tasks.md](file:///home/crochee/workspace/synthia/openspec/changes/extension-points-phase-2/tasks.md).
