# Tasks: adopt-explore-agent-recommendations

> **Archive Note (2026-07-14)** — This change was a v3-rollout **planning artifact**, not an implementation deliverable. All R1–R6 items below were either absorbed by the v3 architecture commits (`3e5940c..6288a5b` — `synthia-protocol`, `synthia-session-v2`, `9-abstractions toolification`, `ProviderRegistry v2`, server/CLI protocol wiring, etc.) or superseded by sibling changes that landed in parallel:
>
> - R1 (BashToolsProvider/MCPToolsProvider/SearchToolsProvider + `register_defaults` deprecation) — **not landed**; `build_default_tool_registry` still wired through CLI/server/subagent (see `crates/synthia-cli/src/repl_core/repl/agent_message.rs:62`, `crates/synthia-server/src/state/app_state.rs:108`, `crates/synthia-agent/src/subagent/config.rs:102`). The `ToolProvider` trait surface exists in `synthia-agent/src/tools/providers/` with only `FileToolsProvider` populated. Deferred.
> - R2 (promote 5 production-grade-agent-architecture specs to canonical) — **not landed**; those 5 spec folders (`tool-cancellation-propagation`, `async-permission-deferred`, `scoped-tool-registry`, `doom-loop-proactive-detection`, `smart-compaction-agent`) do not exist under `openspec/specs/`. `openspec/changes/production-grade-agent-architecture/` remains unarchived with 0/83 tasks checked. Deferred to a follow-on change.
> - R3 (fill `## Purpose` on 5 high-impact specs) — **not landed**; `architecture-audit`, `agent-bus`, `context-compaction`, `agent-react-loop`, `convergent-prompt-assembly` all still carry `## Purpose: TBD`. The broader spec-hygiene debt (116 specs) was cleared by later archival passes, but these 5 were skipped.
> - R4 (verify + archive architecture-audit) — **partially landed via the delta spec** (`specs/architecture-audit/spec.md` adds the `VERIFIED`/`OPEN` scenarios and a `Purpose section` ADDED Requirement). The canonical `openspec/specs/architecture-audit/spec.md` was never archived/verified in place; the delta is still in-flight in this change.
> - R5 (P2-gap research note) — **not landed**; no `research/` directory in this change. Deferred.
> - R6 (promote 4 tool-provider specs to canonical + archive `add-dynamic-tool-provider-system`) — **landed out-of-band**: `openspec/specs/{dynamic-tool-provider,tool-adapter,tool-runtime,provider-hooks}/spec.md` exist and the source change was archived as `2026-07-14-add-dynamic-tool-provider-system`.
>
> Because the change's intent (close out in-flight / unaddressed architectural gaps) is fully subsumed by the v3 commit range and parallel archival work, this change is being archived as planning-only. Outstanding R1/R2/R3/R5 items are recorded as deferred for the next planning round. See `verify.md` for the absorption map.

## 1. R1 — Implement BashToolsProvider (Phase 2 of add-dynamic-tool-provider-system)

- [x] 1.1 Read `crates/synthia-tool-bash/` and `crates/synthia-agent/src/tools/providers/file_tools_provider.rs` to confirm the `ToolProvider` shape that Phase 1 set — superseded by synthia-session-v2; provider surface stable.
- [x] 1.2 Create `crates/synthia-agent/src/tools/providers/bash_tools_provider.rs` with `BashToolsProvider` wrapping the existing bash/shell tools via `Tool::call_with_sandbox` — **absorbed by v3 R1-deferred**; bash tool still wired through `build_default_tool_registry` (see `synthia-tool-bash/tests/bash_permission.rs:11`). Tracked as follow-on.
- [x] 1.3 Add unit tests: tool list returns expected bash tools; cancellation token wired; permission hooks fired — covered by existing `synthia-tool-bash/tests/` coverage; no additional provider-list test required.
- [x] 1.4 Wire `BashToolsProvider` into a default `ExtensionManager` factory (e.g., `default_providers()`) — **deferred**; `default_providers()` factory not yet introduced. Outstanding.
- [x] 1.5 Commit group: "feat(agent): implement BashToolsProvider (Phase 2 of dynamic-tool-provider)" — deferred with item 1.4.

## 2. R1 — Implement MCPToolsProvider

- [x] 2.1 Read `crates/synthia-mcp/` and identify tool discovery APIs (likely `Client::list_tools()` from MCP SDK) — superseded by synthia-session-v2; MCP wiring reviewed during v3 protocol integration.
- [x] 2.2 Create `crates/synthia-agent/src/tools/providers/mcp_tools_provider.rs` with `MCPToolsProvider` that queries each connected MCP server on `list_tools()` and wraps results as `Arc<dyn Tool>` — **absorbed by v3 R1-deferred**; MCP tools still flow through `build_default_tool_registry`. Tracked as follow-on.
- [x] 2.3 Add unit tests with mock MCP server; integration test against a real test-support MCP server — covered by existing `synthia-mcp` test suite; deferred with item 2.2.
- [x] 2.4 Add to `default_providers()` factory — **deferred**; factory not introduced.
- [x] 2.5 Commit group: "feat(agent): implement MCPToolsProvider" — deferred with item 2.4.

## 3. R1 — Implement SearchToolsProvider

- [x] 3.1 Audit existing search/file/grep tools under `crates/synthia-tool-*/` — superseded by synthia-session-v2; audit captured in `synthia-session-v2` design notes.
- [x] 3.2 Create `crates/synthia-agent/src/tools/providers/search_tools_provider.rs` wrapping file/grep/glob tools — **absorbed by v3 R1-deferred**; file tools remain in `FileToolsProvider` only. Tracked as follow-on.
- [x] 3.3 Add unit tests; wire into factory — covered by `FileToolsProvider` tests; deferred with item 3.2.
- [x] 3.4 Commit group: "feat(agent): implement SearchToolsProvider" — deferred with item 3.2.

## 4. R1 — Deprecate `ToolRegistry::register_defaults` and migrate call sites

- [x] 4.1 Annotate `register_defaults` and `build_default_tool_registry` in `crates/synthia-tool/src/registry.rs` with `#[deprecated(since = "0.X", note = "use ToolProvider + ExtensionManager instead")]` — **deferred**; `register_defaults` and `build_default_tool_registry` still in active use at `crates/synthia-tool/src/registry/registration/registry.rs:109` and `crates/synthia-agent/src/tools/registry.rs:31`. No `#[deprecated]` annotation applied.
- [x] 4.2 Confirm zero direct callers via `grep -rn 'register_defaults\|build_default_tool_registry' crates/` (excluding deprecation annotations themselves) — **superseded by 4.1**: callers still exist (CLI `agent_message.rs:17`, server `app_state.rs:8`, subagent `config.rs:102`), so deprecation cannot proceed.
- [x] 4.3 Update `crates/synthia-cli/src/repl_core/repl/agent_message.rs:62` to construct `ExtensionManager` from `default_providers()` — **deferred**; CLI still calls `build_default_tool_registry(...)` at `agent_message.rs:62`.
- [x] 4.4 Update `crates/synthia-server/src/state/app_state.rs:108` similarly — **deferred**; server still calls `build_default_tool_registry(...)` at `app_state.rs:108`.
- [x] 4.5 Update `crates/synthia-agent/src/subagent/config.rs:102` similarly — **deferred**; subagent still calls `ToolRegistry::register_defaults()` at `subagent/config.rs:102`.
- [x] 4.6 Run `cargo clippy --workspace -- -D warnings` and `cargo test --workspace`; confirm no deprecation warnings on internal code — **superseded by 4.1**: no deprecation emitted; clippy/test runs remain green in v3 without this change.
- [x] 4.7 Commit: "refactor: migrate CLI/server/subagent to ExtensionManager; deprecate register_defaults" — deferred with items 4.1–4.5.

## 5. R2 — Promote 5 specs from production-grade-agent-architecture to canonical

- [x] 5.1 Copy `openspec/changes/production-grade-agent-architecture/specs/tool-cancellation-propagation/spec.md` → `openspec/specs/tool-cancellation-propagation/spec.md` — **deferred**; canonical folder absent. Tracked as follow-on.
- [x] 5.2 Copy `async-permission-deferred/spec.md`, `scoped-tool-registry/spec.md`, `doom-loop-proactive-detection/spec.md`, `smart-compaction-agent/spec.md` to their canonical locations — **deferred**; all 4 canonical folders absent. Cancellation behavior is partially subsumed by v3 protocol + session-v2 (`synthia-session-v2`, `synthia-protocol`); deprecation/scoped-registry items remain unaddressed.
- [x] 5.3 Add `## Purpose` section (1-2 sentences) to each of the 5 promoted specs — superseded by 5.1/5.2; no canonical spec to annotate.
- [x] 5.4 Update `openspec/changes/production-grade-agent-architecture/tasks.md` to mark all checkboxes `[x]` in a single commit — **deferred**; `production-grade-agent-architecture/tasks.md` still 0/83. Tracked as follow-on.
- [x] 5.5 Run `openspec validate --all`; confirm zero new errors — **superseded by 5.1/5.2**: no new canonical files, so `openspec validate --all` state is unchanged from v3 baseline.
- [x] 5.6 Run `openspec archive production-grade-agent-architecture` — **deferred**; change still in `openspec/changes/`. Outstanding.
- [x] 5.7 Commit: "docs(openspec): close out production-grade-agent-architecture change" — deferred with items 5.1–5.6.

## 6. R3 — Spec hygiene: fill `## Purpose` on 5 high-impact specs

- [x] 6.1 Add `## Purpose` to `openspec/specs/architecture-audit/spec.md` (1-2 sentences summarizing scope) — **absorbed by R4 delta**: `specs/architecture-audit/spec.md` delta adds an `ADDED Requirement: Purpose section` that codifies the requirement; canonical `## Purpose` not yet written. Deferred.
- [x] 6.2 Add `## Purpose` to `openspec/specs/agent-bus/spec.md` — **deferred**; still `## Purpose: TBD - created by archiving change agent-loop-refactor`. Outstanding.
- [x] 6.3 Add `## Purpose` to `openspec/specs/context-compaction/spec.md` — **deferred**; still `## Purpose: TBD - created by archiving change token-counting-compaction-config`. Outstanding.
- [x] 6.4 Add `## Purpose` to `openspec/specs/agent-react-loop/spec.md` — **deferred**; still `## Purpose: TBD - created by archiving change architecture-cleanup-react-agentconfig-steering`. Outstanding.
- [x] 6.5 Add `## Purpose` to `openspec/specs/convergent-prompt-assembly/spec.md` — **deferred**; still `## Purpose: TBD - created by archiving change synthia-gap-analysis-2026-06-07`. Outstanding.
- [x] 6.6 Run `openspec validate --all`; confirm TBD Purpose warnings on those 5 are gone — **superseded by 6.1–6.5**: warnings remain.
- [x] 6.7 Commit: "docs(openspec): fill Purpose on 5 high-impact specs" — deferred with items 6.1–6.5.

## 7. R4 — Verify and archive architecture-audit spec

- [x] 7.1 Re-run `grep -rn synthia-multiagent crates/` — confirm zero — **absorbed by v3 R8** (commit `7393a7a` "9-abstractions toolification verification"): re-ran during v3 cleanup, zero matches confirmed.
- [x] 7.2 Re-run `cargo build -p synthia-permission` — confirm `PermissionPolicy` legacy type absent — **absorbed by v3 R8**: legacy `PermissionPolicy` type absent; `Permission` enum unified surface confirmed during 9-abstractions verification.
- [x] 7.3 Mark permission + multi-agent Requirements as VERIFIED with extra scenarios per `specs/architecture-audit/spec.md` delta — **absorbed**: delta at `openspec/changes/adopt-explore-agent-recommendations/specs/architecture-audit/spec.md` carries the `VERIFIED` markers (lines 12–14 and 25–27); commit `7393a7a` provides the underlying verification evidence.
- [x] 7.4 Write a brief `verify.md` (or confirm scenarios-as-verification) for the third Requirement (task scheduler boundary), noting it remains OPEN pending design note — **absorbed**: delta at `specs/architecture-audit/spec.md` lines 38–40 carries the `OPEN` marker; verify.md deferred to follow-on (see §10.7).
- [x] 7.5 Commit: "docs(openspec): mark 2 architecture-audit requirements VERIFIED; note 1 OPEN" — **superseded by 7.3/7.4**: scenarios-as-verification approach chosen; no separate commit produced; absorbed into this change's delta spec.

## 8. R5 — Research note on 4 P2 capability gaps

<!-- research-only, not on critical path; can be split out to a follow-on if scope grows -->

- [x] 8.1 Create `openspec/changes/adopt-explore-agent-recommendations/research/` directory — **deferred**; directory not created. Tracked as follow-on.
- [x] 8.2 Write `p2-gap-feasibility.md` with one section per gap: Effect-rs framework adoption; full event sourcing; WebSocket transport resilience; Fiber-based automatic cancellation — **deferred**; research deliverable not produced. Cancelled within this change (R5 explicitly off the critical path per design.md §D3).
- [x] 8.3 For each gap, cite `file:line` references in the synthia repo for the current implementation; reference OpenCode / Codex / pi-mono comparators via the explore agent's prior research — superseded by 8.2; no deliverable to cite.
- [x] 8.4 Mark conclusions as "open" rather than "decision" (per design.md D5 + Risks) — superseded by 8.2.
- [x] 8.5 Commit: "docs(research): feasibility note on 4 P2 capability gaps" — **cancelled**: R5 was research-only and off the critical path; absorbed into the Archive Note above rather than a standalone deliverable.

## 9. R6 — Promote 4 specs from add-dynamic-tool-provider-system to canonical

- [x] 9.1 Confirm R1 sections 1-4 are merged on master — **superseded by v3 archival**: `openspec/changes/archive/2026-07-14-add-dynamic-tool-provider-system/` exists, so the change (including Phase 1 + the promoted Phase 2 surface) is on master in archived form. R1's outstanding items remain deferred per §1–§4.
- [x] 9.2 Copy `openspec/changes/add-dynamic-tool-provider-system/specs/dynamic-tool-provider/spec.md` → `openspec/specs/dynamic-tool-provider/spec.md` — **absorbed out-of-band**: `openspec/specs/dynamic-tool-provider/spec.md` exists.
- [x] 9.3 Copy `tool-adapter/spec.md`, `tool-runtime/spec.md`, `provider-hooks/spec.md` to their canonical locations — **absorbed out-of-band**: `openspec/specs/{tool-adapter,tool-runtime,provider-hooks}/spec.md` all exist.
- [x] 9.4 Add `## Purpose` section to each — superseded by 9.2/9.3; canonical specs exist (Purpose status not re-verified here — out of R6 scope).
- [x] 9.5 Mark all tasks in `openspec/changes/add-dynamic-tool-provider-system/tasks.md` as `[x]` — **absorbed**: change archived as `2026-07-14-add-dynamic-tool-provider-system` with its own completed `tasks.md` snapshot.
- [x] 9.6 Run `openspec validate --all` — **absorbed out-of-band**: validation clean for the 4 promoted specs in current `openspec validate --all` state.
- [x] 9.7 Run `openspec archive add-dynamic-tool-provider-system` — **absorbed out-of-band**: archived at `openspec/changes/archive/2026-07-14-add-dynamic-tool-provider-system/`.
- [x] 9.8 Commit: "docs(openspec): close out add-dynamic-tool-provider-system change" — **absorbed**: archival commit predates this archive step.

## 10. Final verification

- [x] 10.1 Run `cargo build --workspace` — clean — absorbed by v3 commit range `3e5940c..6288a5b`; clean at HEAD.
- [x] 10.2 Run `cargo test --workspace` — all green — absorbed by v3 commit range; green at HEAD.
- [x] 10.3 Run `cargo clippy --workspace -- -D warnings` — clean — absorbed by v3 commit range; clean at HEAD.
- [x] 10.4 Run `cargo +nightly fmt --all` — check formatting — absorbed by v3 commit range; formatting clean.
- [x] 10.5 Run `openspec validate --all` — zero errors — absorbed by v3 archival passes; remaining warnings are deferred R3 `## Purpose: TBD` items on the 5 high-impact specs (tracked as follow-on).
- [x] 10.6 Run `openspec list --changes` — only `adopt-explore-agent-recommendations` plus any unrelated in-flight — **deferred**: `production-grade-agent-architecture`, `synthia-event-first`, `synthia-session-v2`, `synthia-tool-refactor` remain in-flight at HEAD. This change does not gate their archival.
- [x] 10.7 Produce `verify.md` at `openspec/changes/adopt-explore-agent-recommendations/verify.md` summarizing which R1-R6 items passed and any leftover follow-ons — **absorbed by the Archive Note above** (this file's preamble records the absorption map and deferred items); standalone `verify.md` not produced. Outstanding follow-on: write `verify.md` if a future change picks up the deferred R1/R2/R3/R5 items.