# Design: adopt-explore-agent-recommendations

## Context

Synthia has 3 in-flight OpenSpec changes (`openspec/changes/`) and 95 canonical specs (`openspec/specs/`). Two of the in-flight changes are at or near completion but never archived:

- `production-grade-agent-architecture` — 5 capabilities (tool cancellation propagation, async permission deferred, scoped tool registry, doom-loop proactive detection, smart compaction agent) were designed and merged to master in 8 commits (`aa5b583` through `e2d8715`). The change's `tasks.md` is stale (0/83 boxes checked) and the 5 change-local specs sit at `openspec/changes/production-grade-agent-architecture/specs/*/spec.md` instead of canonical `openspec/specs/`.
- `add-dynamic-tool-provider-system` — `ToolProvider` trait, `ExtensionManager`, `StaticToolAdapter`, `ToolRuntime`, `FileToolsProvider` shipped. 3 of 4 `ToolProvider` implementations (`BashToolsProvider`, `MCPToolsProvider`, `SearchToolsProvider`) and the `register_defaults` deprecation remain.

71 of 95 canonical specs carry `## Purpose: TBD` boilerplate that the OpenSpec tooling flags. `architecture-audit` spec's 3 Requirements are code-satisfied but unverified.

This change bundles closing these gaps plus a research note on the 4 P2 capability gaps that were deferred during `production-grade-agent-architecture`'s design but never enumerated in writing.

## Goals / Non-Goals

**Goals:**
- Complete Phase 2 of `add-dynamic-tool-provider-system` (`BashToolsProvider`, `MCPToolsProvider`, `SearchToolsProvider`, deprecation, call-site migration).
- Archive `production-grade-agent-architecture` with promoted canonical specs.
- Archive `add-dynamic-tool-provider-system` (after R1) with promoted canonical specs.
- Fill `## Purpose` sections on the 5 highest-impact specs.
- Verify and archive `architecture-audit` spec.
- Produce a research note on the 4 P2 gaps.

**Non-Goals:**
- Implementing the 4 P2 gaps themselves (effect-rs, event sourcing, WebSocket resilience, Fiber cancellation). R5 produces a research note only.
- Re-running or auditing completed/archived changes (`p0-trait-review-remediation`, `p1-skillprovider-remediation`, `p2-trait-cleanup`, `agent-bug-fix-and-dedup`, `synthia-optimization`).
- Adding new `ToolProvider` traits or extending the trait surface (covered by R1 implementations, not new capabilities).

## Decisions

### D1: Single bundled change vs. one-change-per-recommendation
- **Choice**: Single bundled superpowers-bridge change.
- **Rationale**: Recommendations share the theme "close out in-flight / unaddressed architectural gaps." Per-recommendation changes would inflate the apply surface and fragment review.
- **Considered alternatives**:
  - Six separate changes → rejected (overhead, looser coupling than warranted for the recommendation sizes)
  - Two changes (code vs. paperwork) → rejected (paperwork depends on code completion, awkward fork)

### D2: Schema = `superpowers-bridge`
- **Choice**: Use superpowers-bridge (auto-selected by `openspec new change`).
- **Rationale**: Provides `brainstorm.md` raw-capture slot and `plan.md` micro-task decomposition via superpowers skills.

### D3: R5 is research deliverable in same change, not separate follow-on
- **Choice**: Embed P2-gap research as a sub-task delivering `research/p2-gap-feasibility.md`.
- **Rationale**: Keeps context intact; delivers faster; reviewer reads 1 doc, not 2.
- **Considered alternatives**:
  - Separate change → rejected (research note depends on R1/R2 having shaken out merge conflicts first)

### D4: `architecture-audit` spec modified twice (R3 + R4)
- **Choice**: Sequence R3 (add `## Purpose` + minor) → apply → R4 (add VERIFIED scenarios).
- **Rationale**: Two distinct edits to the same spec. Separating concerns keeps each diff reviewable.
- **Sequencing**: R3 lands first as a docs-only commit. R4 lands in a separate commit after grep verification.

### D5: No new delta spec emission for R6 promotion
- **Choice**: R6 promotion copy-pastes change-local specs to canonical `openspec/specs/` as-is. Treat promotion as an archival mechanic, not a capability-modifying change.
- **Rationale**: The change-local specs were already validated when `add-dynamic-tool-provider-system` was created. Re-validating identical content as a delta-spec write is duplicate work.
- **Considered alternatives**:
  - Emit MODIFIED delta → rejected (the canonical spec is empty until promotion; modifying "nothing" is meaningless)

### D6: `ToolRegistry::register_defaults` deprecation path
- **Choice**: Annotate with `#[deprecated(since = "...", note = "...")]`; retain the function as a thin wrapper that constructs an `ExtensionManager` and registers the existing defaults. New CLI/server paths use the manager directly.
- **Rationale**: Backward compat for any external CLI tooling that calls `register_defaults` directly. Allows one minor version of overlap before removal.
- **Considered alternatives**:
  - Hard removal → rejected (breaks consumers immediately)

### D7: Promotion order
- **Choice**: Archive `production-grade-agent-architecture` first (R2), then `add-dynamic-tool-provider-system` (after R1, via R6).
- **Rationale**: R2 is paperwork only and unblocks `openspec validate --all` sooner. R6 depends on R1's code being on master.

## Risks / Trade-offs

- **[Risk] Bundled change hides regressions** → Mitigation: tasks structured so each recommendation is independently committable and reviewable; `git range-diff master..HEAD` per recommendation before archiving.
- **[Risk] R1 deprecation breaks an external consumer that bypasses CLI/server** → Mitigation: `#[deprecated]` annotation; docs note removal in next minor; provide feature flag `legacy-default-registry` for one minor.
- **[Risk] P2 research note (R5) gets out of date fast** → Mitigation: cap canonical claims to file:line references inside the synthia repo; mark opinions as "open" rather than "decision."
- **[Trade-off] Reduced atomicity per recommendation** → accepted for compact verify surface; mitigated by per-recommendation commit boundaries.
- **[Trade-off] Promoting specs from change-local to canonical without re-validation** → accepted because the change-local specs were already validated at change creation; mitigate by running `openspec validate --change add-dynamic-tool-provider-system` and `... production-grade-agent-architecture` before promoting.

## Migration Plan

**Order of operations (critical path):**
1. R1 code: implement `BashToolsProvider` → `MCPToolsProvider` → `SearchToolsProvider` → deprecate `register_defaults` → migrate CLI/server call sites. Each with tests.
2. R2 paperwork: copy 5 specs to canonical, fill `## Purpose`, run `openspec validate --all`, mark tasks.md, `openspec archive production-grade-agent-architecture`.
3. R3 docs-only: add `## Purpose` to 5 specs in one commit.
4. R4 verification: add `VERIFIED` scenarios to `architecture-audit`, run `openspec archive architecture-audit` (spec-only archive, not change archive).
5. R5 research: write `research/p2-gap-feasibility.md`; mark as `<!-- research-only -->` in tasks.md.
6. R6 paperwork: copy 4 specs to canonical, fill `## Purpose`, validate, `openspec archive add-dynamic-tool-provider-system`.

**Rollback strategy:** Each recommendation is independently revertible. Reverting one does not impact others.

**Acceptance criteria:** `openspec validate --all` passes with zero `TBD` Purpose matches among the 5 R3 specs and zero `## MODIFIED Requirements` drift; `openspec list --changes` returns only `adopt-explore-agent-recommendations` plus any post-merge follow-ons; `cargo test --workspace` passes.

## Open Questions

1. Does `register_defaults` removal need an RFC, or is deprecation-only adequate for one minor? (Tie-breaker: grep for direct callers in current HEAD — if zero, deprecation-only is fine.)
2. For R3, which 5 specs deserve priority — `architecture-audit`, `agent-bus`, `context-compaction`, `agent-react-loop`, `convergent-prompt-assembly`? Open to substitution if any spec is later touched by a concurrent change.
3. R5 P2 ordering: which of the 4 P2 gaps (effect-rs, event sourcing, WebSocket resilience, Fiber cancellation) is highest priority for the next planning round? Research note will recommend but not decide.
4. Should `architecture-audit` be archived as a spec (R4) or as a change? Spec-only archive if the spec body itself is the deliverable; change archive requires a change directory with proposal/tasks — heavier. Recommend spec-only archive.
