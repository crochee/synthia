<!--
Raw capture of superpowers:brainstorming output.

本檔原樣捕捉 brainstorming skill 的產出，不強制結構。
Skill 的自然產出通常是 decision log 格式（背景 → 決議鏈 Q1-Qn → 設計取捨），
但依對話內容可能有不同組織方式。

design.md 從本檔萃取並重新整理為結構化設計文件。

不要將本檔的內容複製到 design.md — design.md 是獨立的重組產物，
兩者互補但不重疊。
-->

# Brainstorm: extension-points-phase-2

**Date:** 2026-07-12 (Asia/Shanghai)
**Change:** `extension-points-phase-2`
**Schema:** `superpowers-bridge`
**Mode:** Authoring (extending an already-validated scope)

---

## Background

`tool-abstraction-and-extensibility` (archived 2026-07-12) delivered **21
extension points across 2 scopes** (Agent Loop + Tool) per the
`extension-point-matrix/spec.md` requirement of 64 total points across 10
scopes. Phase 4 of that change (43 points across 8 remaining scopes) was
deferred to a follow-up change. This change implements that follow-up.

**Existing reusable assets** (no need to re-design):
- `extension_context.rs` — three-state lifecycle (Loading/Active/Stale)
- `extension_points/agent_loop.rs` — observe-only registry pattern
- `extension_points/tool.rs` — `Action<T>` mutation pattern + wildcard matching
- `extension_manager.rs` — O(1) provider registration with version counter
- 47 tests in `dynamic_provider` covering state machine + concurrency

**Existing hard constraints** (from `project_memory.md` and
`.trae/rules/agent_rule.md`):
- P1 — KV-cache prefix consistency (no extensions may mutate the prefix hash)
- P6 — Distrust by Default (Permission/DoomLoop fail-closed, not fail-open)
- P9 — Observability (every `fire` and every state transition emits OTel)

---

## Decision chain

### Q1: How to organize the 8 new scopes' code?

Three options:

| Option | Description | Trade-off |
|---|---|---|
| **A. One module per scope** | 8 new files in `extension_points/` | Symmetric with Phase 3 (agent_loop.rs, tool.rs). Easiest to navigate. **Recommended.** |
| B. Group by lifecycle | 3 files: `request-time.rs`, `response-time.rs`, `lifecycle.rs` | Tighter coupling, but harder to reason about cross-cutting concerns (Permission spans all 3). |
| C. Single mega-file | One `extension_points/part2.rs` with 43 points | Faster initial write, but a 2,000-line file violates the CLAUDE.md "smaller, well-bounded units" guideline. |

**Decision: A** — one module per scope, mirroring Phase 3.

### Q2: Mutation vs observation pattern per scope?

Phase 3 introduced two patterns:
- **Observe-only** (Agent Loop): handler returns `()`. Used for telemetry, logging, audit.
- **Mutation** (Tool): handler returns `Action<T>` (Proceed|Modify|Skip). Used for transforming data flowing through the agent.

For Phase 4, classify each scope:

| Scope | Pattern | Reason |
|---|---|---|
| LLM | **Mutation** | `chat.params` must mutate the params; `system_prompt.transform` must rewrite the prompt |
| Context | **Mutation** | `context.compact.replace` must replace the compacted messages; `context.message_filter` must filter |
| Permission | **Mutation (constrained)** | `permission.ask` can add to deny list (more restrictive only); `permission.notify` is observe-only |
| Provider | **Mutation** | `provider.register` adds a provider; `provider.fallback` selects a fallback chain |
| Plugin Lifecycle | **Mutation (state-bound)** | Reuses `ExtensionContext` semantics; transitions trigger the events |
| Event Bus | **Observe-only** | Pub/sub; no data flow to mutate |
| Session Tree | **Mutation (write-bound)** | `session.entry.append` may rewrite; `session.branch.create` may redirect |
| Output/UI | **Mutation (intercept-bound)** | `ui.dialog.*` and `output.format` mutate the user's view; `ui.render.component` is observe-only |

**Decision: Apply the same two-pattern framework from Phase 3, but introduce
a third: "Mutation (state-bound)" for Plugin Lifecycle, which delegates to
`ExtensionContext` rather than introducing a new state machine.**

### Q3: How does the Permission scope interact with the "permission-fail-closed" hard rule?

This is the most subtle design question. The extension point `permission.ask`
sounds like it could let an extension override the user's permission decision.
That would violate the project's hard rule: "Permission policy must default
to 'AskUser' (fail-closed) instead of 'Allow' (fail-open)".

Three options:

| Option | Description | Trade-off |
|---|---|---|
| **A. Extension can only ADD to deny list** | Extension returns `Action<PermissionDecision>` where the only allowed transitions are: `Ask` → `Deny` (never `Allow`); `Allow` → `Allow` (no change); `Deny` → `Deny` (no change) | Preserves fail-closed. Extensions are more-restrictive-only. **Recommended.** |
| B. Extension can fully override | Extension returns any `PermissionDecision` | Violates P6 fail-closed. **Rejected.** |
| C. No `permission.ask` extension point | Remove it; keep only `permission.notify` (observe-only) | Loses a useful extension capability (e.g., blacklist additions from a security plugin). **Too restrictive.** |

**Decision: A** — extension can only ADD to the deny list. This is implemented
as a typed return value `Action<PermissionDecision>` where the underlying
function is a `|dec: PermissionDecision| -> PermissionDecision` that enforces
"can only get more restrictive". The compiler doesn't enforce this, but the
permission checker's logic does (with a test that any attempt to weaken is
overridden to `AskUser`).

### Q4: How does the Context scope interact with the P1 prefix hash?

P1 says: "no extensions may mutate the prefix hash". The Context scope has
extension points like `context.message_filter` and `context.compact.replace`
that obviously mutate the message stream. How to reconcile?

Three options:

| Option | Description | Trade-off |
|---|---|---|
| **A. Hooks fire BEFORE prefix snapshot, then snapshot captures the post-hook state** | Extension runs first, snapshot reflects post-hook state, hash changes but the "before" baseline is preserved | Clean. Snapshot just gets a different hash. **Recommended.** |
| B. Hooks fire AFTER prefix snapshot, so the prefix hash stays the same | Extension can't affect what was sent to the LLM | Means hooks are post-hoc — not useful. **Rejected.** |
| C. No `message_filter` extension point | Lose capability | Too restrictive. **Rejected.** |

**Decision: A**. The prefix hash is computed AFTER all Context scope hooks
fire. The hash is allowed to change between calls (that's the point of
caching invalidation), but the agent loop is required to re-snapshot after
the hook chain. This is consistent with how `compact_context_tool` works
(Phase 2.1.1: real work happens in main_loop, not in the Tool).

### Q5: Where does the Plugin Lifecycle scope's `extension.hot_swap` fit?

`extension.hot_swap` is interesting because the existing `ExtensionContext`
state machine (Loading/Active/Stale) doesn't model "swap one extension for
another mid-session". Three options:

| Option | Description | Trade-off |
|---|---|---|
| **A. Hot-swap = load + bind + invalidate old atomically** | Three step transition: queue new extension → invalidate old → bind new | The state machine handles this; no new states needed. **Recommended.** |
| B. Add a `Swapping` state to `ExtensionContext` | More states, more tests | Over-engineering for what is effectively a 3-event sequence. **Rejected.** |
| C. Hot-swap = extension.unload + extension.load (two events) | Simpler | Doesn't handle the in-flight calls; would cause inconsistencies. **Rejected.** |

**Decision: A**. Hot-swap fires `extension.load` (new), then
`extension.invalidate` (old), then `extension.bind` (new). The state
machine treats this as Loading → Active → Stale + Loading → Active, with
the Old active state being retained via `last_active` for diagnostics.

### Q6: What about `output.format` and `ui.render.component`?

`output.format` mutates the Tool output (similar to `tool.execute.after` from
Phase 3 Tool scope). `ui.render.component` is purely a UI hook (observe-only).
Should they share a registry or be separate?

Three options:

| Option | Description | Trade-off |
|---|---|---|
| **A. Both in Output/UI scope, single registry** | One `OutputExtensionRegistry` | Symmetric with Phase 3. **Recommended.** |
| B. Separate Output and UI registries | Two files, two registries | Splits conceptually related concerns. **Rejected.** |
| C. Reuse the Tool scope's `tool.execute.after` | Add `output.format` to Tool scope | Cross-scope coupling. Violates `extension-point-matrix/spec.md` "10 scopes" requirement. **Rejected.** |

**Decision: A**. Single `OutputExtensionRegistry` for the entire Output/UI
scope. Handler types are differentiated by point name (e.g.,
`ui.dialog.select` vs `output.format`).

### Q7: Implementation order across 4 rounds?

Each round = 1-2 scopes = 1 logical commit. Order by P-risk and usage:

| Round | Scopes | Points | Why this order |
|---|---|---|---|
| **1** | Context + LLM | 7 + 8 = 15 | Most-used, P1 interactions — get prefix-snapshot semantics right first |
| **2** | Permission + Provider | 5 + 4 = 9 | Security/cost surface — fail-closed semantics need test coverage before plugins use them |
| **3** | Event Bus + Plugin Lifecycle | 4 + 6 = 10 | Meta-observability — useful for the prior 34 points but not blocking |
| **4** | Session Tree + Output/UI | 5 + 4 = 9 | Last-mile UI concerns — least used in current code |

**Decision: Round 1 first**. Each round = 1 apply cycle, 1 verify, 1 commit
(some rounds may need 2-3 sessions of work).

---

## Design trade-offs

### T1: Synchronous dispatch (re-use Phase 3 decision)

Both observe-only and mutation patterns are dispatched **synchronously** in
registration order. Handler panics are caught via `std::panic::catch_unwind`.

Rationale: matches Phase 3.1-3.4, which the user signed off on. Async
dispatch would require `Pin<Box<dyn Future>>` per handler with ordering
guarantees. Not justified by current use cases.

### T2: Tool `Action<T>` for both scopes that mutate

LLM, Context, Permission, Provider, Session Tree, Output/UI all use
`Action<T>` (Proceed|Modify|Skip). The T varies per scope:

| Scope | T for `Action<T>` |
|---|---|
| LLM | `ChatParams`, `Messages`, `SystemPrompt`, etc. |
| Context | `MessageList`, `CompactionResult`, `TokenBudget` |
| Permission | `PermissionDecision` (constrained to "more restrictive only") |
| Provider | `ProviderConfig`, `FallbackChain` |
| Session Tree | `SessionEntry`, `BranchNavigation` |
| Output/UI | `OutputContent`, `UiDialog` |

This is more type machinery than 4 separate enums, but the consistency is
worth it: every `fire_*` returns `Action<T>` and chains identically.

### T3: Observe-only scopes don't get `Action<T>`

Event Bus, Plugin Lifecycle (state-bound), and `ui.render.component` use
direct handler returns (`()`). The registry type for these scopes is
`Registry<Handler = Arc<dyn Fn(&Event) + Send + Sync>>` (no `Action<T>`
machinery).

### T4: OTel span shape (re-use Phase 3 pattern)

Every `fire` and every state transition emits:
- `extension.hook { point, scope, extension_id, payload_size? }`
- `extension.bind_core { session_id, provider_count }`
- `extension.invalidate { from_state, retained_runtime }`

No new OTel conventions introduced.

### T5: Test strategy (re-use Phase 3 pattern)

Per scope: minimum 2 tests (1 register-fire, 1 wildcard-or-mutation). Plus
2-3 state-machine tests per scope that has one. Total: ~24-30 new tests
across 8 scopes (3 per scope average).

Plus a **shared integration test** at the end (Round 4's validation) that
verifies all 64 points (21 Phase 3 + 43 Phase 4) can be registered without
name collisions across scopes.

---

## Risks

### R1: Permission "more restrictive only" is not compiler-enforced

The compiler can't verify that an extension handler only returns more
restrictive `PermissionDecision` values. We rely on:
- Doc comments stating the rule
- Unit tests that try to weaken and verify the override
- Runtime logging if a handler attempts to weaken

**Mitigation**: A `PermissionExtensibilityGuard` test in Round 2.

### R2: Context hooks can produce an invalid prefix hash

If `context.message_filter` removes a message that was previously part of
the prefix, the next snapshot will have a different hash. This is correct
behavior, but it means the cache may be invalidated more often than
expected. The orchestrator must be careful not to assume the hash is stable
across "no-op" hook fires.

**Mitigation**: Round 1's tests must include a "hook returning Proceed on
unchanged input → no hash change" test.

### R3: Event Bus pub/sub order is not guaranteed

Multiple subscribers to the same event may receive it in different orders
if they were registered on different scopes. The current design says
"registration order within a single scope", but cross-scope ordering is
undefined.

**Mitigation**: Document the ordering guarantee. Don't promise cross-scope
ordering.

### R4: 43 extension points is a lot to review in one PR

The 4 rounds split this into 4 PR-sized chunks, but each round is still
7-15 points. Reviewers may struggle.

**Mitigation**: Each spec file is self-contained; reviewers can review
scope-by-scope within a round. Plus 4-round cadence lets users review +
merge + use + review the next round.

---

## Out of scope (deferred to separate changes)

- `2.2.3` ExternalHookTool (deferred from tool-abstraction)
- `2.3.2` Plugin CLI as Tool (deferred from tool-abstraction)
- `5.x` PluginHookAdapter (Phase 5 of tool-abstraction, separate change)
- `6.x` Integration + E2E (Phase 6 of tool-abstraction, separate change)
- Any per-scope "future scope" extensions (e.g., `output.compress`, `llm.streaming.*`)

---

## Self-review

- ✅ No placeholders. Every extension point is named with typed input/output.
- ✅ No contradictions. R1 (Permission weakening) is documented with a mitigation.
- ✅ Scope check: 43 points fits in 4 apply-rounds, each ~1 week.
- ✅ Ambiguity check: the "more restrictive only" rule is in the spec, with tests.

---

**User approval**: Captured above. Proceeding to author `design.md`, `proposal.md`, `specs/`, `tasks.md`, and `plan.md` in dependency order.
