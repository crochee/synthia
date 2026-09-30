# Design: extension-points-phase-2

> Reorganized from [brainstorm.md](file:///home/crochee/workspace/synthia/openspec/changes/extension-points-phase-2/brainstorm.md).
> This is the structured design (Context / Goals / Decisions / Risks / Migration).
> The brainstorm.md is the raw decision log; design.md is the formal spec.

## Context

The `tool-abstraction-and-extensibility` change (archived 2026-07-12)
delivered 21 of the 64 extension points specified by
[extension-point-matrix/spec.md](file:///home/crochee/workspace/synthia/openspec/specs/extension-point-matrix/spec.md) (the
Agent Loop + Tool scopes, with phase 3 of that change). The remaining
43 points across 8 scopes (LLM, Context, Permission, Provider, Plugin
Lifecycle, Event Bus, Session Tree, Output/UI) are deferred to this change.

**Reusable assets** (from Phase 3, no redesign needed):
- [`ExtensionContext`](file:///home/crochee/workspace/synthia/crates/synthia-agent/src/tools/dynamic_provider/extension_context.rs) — three-state lifecycle (Loading/Active/Stale)
- [agent_loop.rs](file:///home/crochee/workspace/synthia/crates/synthia-agent/src/tools/dynamic_provider/extension_points/agent_loop.rs) — observe-only registry pattern
- [tool.rs](file:///home/crochee/workspace/synthia/crates/synthia-agent/src/tools/dynamic_provider/extension_points/tool.rs) — `Action<T>` mutation pattern + wildcard matching
- [`ExtensionManager`](file:///home/crochee/workspace/synthia/crates/synthia-agent/src/tools/dynamic_provider/extension_manager.rs) — O(1) provider registration
- 47 tests covering state machine + concurrency

**Hard constraints** (must not violate):
- P1 — KV-cache prefix consistency (no extensions may mutate the prefix hash)
- P6 — Distrust by Default (Permission/DoomLoop fail-closed)
- P9 — Observability (every `fire` and every state transition emits OTel)

## Goals

1. **Complete the 64-point matrix** in `extension-point-matrix/spec.md`.
2. **Reuse the Phase 3 patterns** (state machine, `Action<T>`, OTel spans).
3. **Add 4 well-bounded commits** (one per implementation round).
4. **Add ≥24 new tests** covering per-scope + cross-scope behavior.
5. **Document per-point "used by / reserved for"** so future readers know which points are exercised today vs reserved for future.

## Non-Goals

- Phase 5 (PluginHookAdapter) and Phase 6 (Integration + E2E) of the original `tool-abstraction-and-extensibility` plan — separate changes.
- `2.2.3 ExternalHookTool` and `2.3.2 Plugin CLI as Tool` — separate follow-up change.
- Adding new scopes beyond the 10 specified in `extension-point-matrix/spec.md`.

## Architecture

### Module structure

```
crates/synthia-agent/src/tools/dynamic_provider/extension_points/
├── mod.rs                      # re-exports
├── agent_loop.rs               # Phase 3 (12 points)
├── tool.rs                     # Phase 3 (9 points)
├── llm.rs                      # Round 1 (8 points)
├── context.rs                  # Round 1 (7 points)
├── permission.rs               # Round 2 (5 points)
├── provider.rs                 # Round 2 (4 points)
├── event_bus.rs                # Round 3 (4 points)
├── plugin_lifecycle.rs         # Round 3 (6 points)
├── session_tree.rs             # Round 4 (5 points)
└── output_ui.rs                # Round 4 (4 points)
```

Each scope file follows the same template:

```rust
//! <Scope> extension points
//!
//! Implements the <N> extension points from
//! extension-point-matrix/spec.md §<Requirement>.

// 1. Typed event structs (no serde_json::Value)
pub struct XxxInput { ... }
pub struct XxxOutput { ... }

// 2. Registry type
pub struct XxxExtensionRegistry {
    handlers: Arc<DashMap<&'static str, Vec<XxxHandler>>>,
    // ... per-scope state if needed
}

impl XxxExtensionRegistry {
    pub fn new() -> Self { ... }
    pub fn register(&self, point: &'static str, id: String, handler: XxxHandler) { ... }
    pub fn unregister(&self, point: &'static str, id: &str) -> bool { ... }
    pub fn fire(&self, event: &XxxEvent) -> Action<XxxOutput> { ... }
    // ... per-scope methods (e.g., wildcard matching for tool.rs)
}

// 3. Tests (≥2 per scope + per-scope state machine if applicable)
```

### Pattern per scope

| Scope | Pattern | Action<T> | Reason |
|---|---|---|---|
| LLM | Mutation | Yes (typed T) | Most points must rewrite LLM-bound data |
| Context | Mutation | Yes (typed T) | Compaction + filtering are data transformations |
| Permission | Mutation (constrained) | Yes (constrained T) | Fail-closed semantics; can only ADD to deny list |
| Provider | Mutation | Yes (typed T) | Provider registration + fallback chain |
| Plugin Lifecycle | Mutation (state-bound) | No (state machine) | Reuses `ExtensionContext` |
| Event Bus | Observe-only | No | Pub/sub; no data flow to mutate |
| Session Tree | Mutation (write-bound) | Yes (typed T) | Write hooks for entry append + branch create |
| Output/UI | Mutation (intercept-bound) | Yes (typed T) | Most points rewrite user-facing output |

### OTel span shape (re-use Phase 3)

```rust
tracing::info_span!(
    target: "synthia.extension",
    "extension.hook",
    point = point_name,        // e.g. "chat.params"
    scope = scope_name,        // e.g. "llm"
    extension_id = handler_id, // e.g. "my-plugin#0"
)
```

For state-bound scopes (Plugin Lifecycle):
```rust
tracing::info_span!(
    target: "synthia.extension",
    "extension.bind_core" | "extension.invalidate",
    session_id = session_id,
    provider_count = count,
    from_state = "loading" | "active" | "stale",
)
```

## Decisions

### D1: One module per scope

**Rationale**: Symmetric with Phase 3 (agent_loop.rs, tool.rs). Easiest to navigate. Each file is ~200-400 lines, within the "smaller, well-bounded units" guidance from CLAUDE.md.

**Rejected alternatives**:
- Group by lifecycle (request-time/response-time/lifecycle) — splits Permission across 3 files
- Single mega-file — 2,000 lines is unmaintainable

### D2: 3 pattern families (mutation / mutation-constrained / observe-only)

**Rationale**: Two patterns from Phase 3 (mutation via `Action<T>`, observe-only via `Fn(&Event)`) cover most cases. Adding "mutation-constrained" (Permission's fail-closed rule) is a single additional pattern, not 3.

**Rejected alternatives**:
- One pattern for all 8 scopes — loses fail-closed semantics for Permission
- A different pattern per scope (8 patterns) — too much variation, hard to learn

### D3: 4 implementation rounds (commit boundaries)

| Round | Scopes | Points | Commit message |
|---|---|---|---|
| 1 | Context + LLM | 15 | `feat(extension): 15 LLM + Context extension points` |
| 2 | Permission + Provider | 9 | `feat(extension): 9 Permission + Provider extension points (fail-closed)` |
| 3 | Event Bus + Plugin Lifecycle | 10 | `feat(extension): 10 Event Bus + Plugin Lifecycle extension points` |
| 4 | Session Tree + Output/UI | 9 | `feat(extension): 9 Session Tree + Output/UI extension points + 64-point integration test` |

**Rationale**: Each round = 1 logical commit, ≤15 extension points (reviewable in <30 min). Round 1 establishes the P1 (prefix hash) interaction; Round 2 establishes the P6 (fail-closed) interaction; Round 3 is the meta-observability layer; Round 4 closes out + adds the 64-point integration test.

### D4: Permission "more restrictive only" is runtime-enforced, not compiler-enforced

**Rationale**: The compiler can't verify that an extension handler only returns more restrictive `PermissionDecision` values. We rely on:
- Doc comments stating the rule
- Unit tests that try to weaken and verify the override
- Runtime logging if a handler attempts to weaken

The `PermissionExtensibilityGuard` in Round 2 wraps the chain with a `|dec| dec` clamp that downgrades any weakening attempt to `AskUser`.

**Rejected alternative**: Removing `permission.ask` entirely — too restrictive, would block legitimate security plugins.

### D5: Context hooks fire BEFORE prefix snapshot

**Rationale**: Hooks must affect what's sent to the LLM. The snapshot is computed after the hook chain. The hash is allowed to change between calls (that's the point of caching invalidation). The agent loop is required to re-snapshot after the hook chain.

**Rejected alternatives**:
- Hooks after snapshot — pointless, can't affect LLM input
- No `message_filter` — too restrictive, would block legitimate use cases

### D6: Plugin Lifecycle reuses `ExtensionContext` state machine

**Rationale**: `extension.hot_swap` is "load new + invalidate old + bind new" — a 3-step transition that maps to existing Loading→Active→Stale + Loading→Active. No new states needed.

**Rejected alternative**: Add a `Swapping` state to `ExtensionContext` — over-engineering for what is effectively a 3-event sequence.

## Risks

### R1: Permission weakening not compiler-enforced

See D4. Mitigated by `PermissionExtensibilityGuard` test + runtime logging.

### R2: Context hooks can produce an invalid prefix hash

See D5. Mitigated by Round 1 test "hook returning Proceed on unchanged input → no hash change".

### R3: Event Bus pub/sub order is not guaranteed cross-scope

Document the ordering guarantee: "registration order within a single scope". Don't promise cross-scope ordering.

### R4: 43 extension points is a lot to review in one PR

See D3 — 4 rounds split into reviewable chunks. Each spec file is self-contained.

## Migration

### Backward compatibility

- All 21 Phase 3 extension points continue to work unchanged.
- No breaking changes to `ExtensionContext`, `ExtensionManager`, or any existing public API.
- New scope registries are added in parallel — no existing call site is modified in this change.

### Forward compatibility

- The `Action<T>` machinery is generic over `T: Serialize + Deserialize`. Future scopes (if any beyond the 10 specified) can reuse the pattern.
- The `Action<T>` return type's `Skip { reason }` variant gives extensions a way to opt out of an operation (e.g., a Permission extension that says "skip this tool call" — implemented in Round 2).
- OTel span attributes are versioned by `scope` (e.g., `scope = "llm"`), so new scopes don't conflict with old.

### Code base impact (per round)

| Round | New files | Modified files | Net new tests |
|---|---|---|---|
| 1 (LLM + Context) | 2 (llm.rs, context.rs) + 1 spec each | 0 (purely additive) | 6-8 |
| 2 (Permission + Provider) | 2 + 1 spec each | 0 | 4-6 |
| 3 (Event Bus + Plugin Lifecycle) | 2 + 1 spec each | 0 | 4-6 |
| 4 (Session Tree + Output/UI) | 2 + 1 spec each + 1 integration test | 0 | 4-6 |
| Total | 8 + 8 specs + 1 integration | 0 | 18-26 |

**Net new code**: ~2,000-2,500 lines (impl + tests). No deletions. No modifications to existing code.

## Self-Review

- ✅ No placeholders. Every extension point is named with typed input/output.
- ✅ No contradictions. R1 (Permission weakening) is documented with a mitigation.
- ✅ Scope check: 43 points fits in 4 apply-rounds, each ~1 session.
- ✅ Ambiguity check: the "more restrictive only" rule is in the spec, with tests.
- ✅ Backward compat: existing 21 points unchanged.
- ✅ Hard constraints preserved: P1 (Context scope, D5), P6 (Permission scope, D4), P9 (every fire and transition, OTel shape section).

---

**Proceeding to**:
1. [proposal.md](file:///home/crochee/workspace/synthia/openspec/changes/extension-points-phase-2/proposal.md) — what & why
2. [specs/](file:///home/crochee/openspec/changes/extension-points-phase-2/specs) — 8 capability specs
3. [tasks.md](file:///home/crochee/openspec/changes/extension-points-phase-2/tasks.md) — implementation steps
4. [plan.md](file:///home/crochee/openspec/changes/extension-points-phase-2/plan.md) — micro-task plan
