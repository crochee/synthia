# Brainstorm: adopt-explore-agent-recommendations

> Raw capture of superpowers:brainstorming output for this change.
> Preserves decision-log style (background → decision chain → trade-offs).
> design.md reorganizes this content into structured sections.

## Background (from explore-agent scan of repo state)

The user's original intent was the Chinese phrase "采纳推荐" ("adopt recommendations"). After clarification, scope was set to **agent-driven exploration**: dispatch an explore subagent to scan the repo for unaddressed recommendations and structure them into a single OpenSpec change.

The explore agent returned 6 ranked recommendations anchored in:

- `openspec/changes/production-grade-agent-architecture/` — code fully shipped (5 capabilities committed to master: `aa5b583`, `c7d576c`, `51d58fa`, `c568304`, `e230937`, `6b6af39`, `154f029`, `e2d8715`), but change never archived and specs never promoted to `openspec/specs/`.
- `openspec/changes/add-dynamic-tool-provider-system/` — Phase 2 incomplete (0/34 tasks). Phase 1 shipped (`ToolProvider` trait, `ExtensionManager`, `StaticToolAdapter`, `ToolRuntime`, `FileToolsProvider`). Pending: `BashToolsProvider`, `MCPToolsProvider`, `SearchToolsProvider`, `register_defaults` deprecation, CLI/server call-site migration.
- `openspec/specs/architecture-audit/spec.md` — 3 verification-only Requirements, all satisfied in code but never archived (no `verify.md`).
- 71 specs with `## Purpose: TBD - created by archiving change X` boilerplate (spec hygiene debt).
- 4 P2 capability gaps from the production-grade `brainstorm.md` (9 gaps identified, only 5 P0/P1 selected) — never enumerated in writing.

The user's clarified intent: **let the AI autonomously enumerate and prioritize the recommendations**, then adopt them as a single bundled change.

## Decision chain

### Q1: Single bundled change vs. one-change-per-recommendation?

- **Option A**: One superpowers-bridge change `adopt-explore-agent-recommendations` with sub-tasks per recommendation.
- **Option B**: Six separate changes (R1-R6).
- **Decision**: **A** — the recommendations share a theme (close out in-flight / unaddressed architectural gaps) and are small/medium effort. A single change keeps the verification surface compact and matches the user's "let AI propose, adopt all" intent. R5 (research-only spike) is included as a deliverable within this change rather than spun out.

### Q2: Schema choice?

- `superpowers-bridge` was auto-selected by `openspec new change`. Use as-is to allow `brainstorm.md` raw capture and `plan.md` decomposition via superpowers skills.

### Q3: Which recommendations to include in scope (apply path) vs. scope as research deliverable (R5)?

- **Apply**: R1 (complete dynamic-tool-provider Phase 2), R2 (archive production-grade-agent-architecture), R3 (fill `## Purpose` sections on 5 highest-impact specs), R4 (verify & archive architecture-audit), R6 (after R1, promote tool-provider specs).
- **Research deliverable only**: R5 (4 P2 capability gaps research note) — produces a Markdown feasibility note inside this change but does not commit implementation.
- Rejected: separate `p2-trait-cleanup` / `agent-bug-fix-and-dedup` re-runs (already archived).

### Q4: Spec delta strategy?

- R1 modifies no `openspec/specs/*/spec.md` (its specs are change-local in `openspec/changes/add-dynamic-tool-provider-system/specs/`). Apply phase produces the code; archive phase (R6) copies those specs to canonical `openspec/specs/`.
- R2 is pure paperwork — no new requirements; the archive step promotes existing change-local specs to canonical.
- R3 modifies `openspec/specs/<kebab-name>/spec.md` for 5 specific specs (architecture-audit, agent-bus, context-compaction, agent-react-loop, convergent-prompt-assembly) by adding a `## Purpose` section. Treated as a requirements-level documentation change → emit delta specs.
- R4 modifies `openspec/specs/architecture-audit/spec.md` to mark each Requirement as **VERIFIED** in a new scenario. (Re-uses the spec that R3 also touches — coordinated.)
- R5 introduces **NEW** specs only if the research note identifies candidates — handled as a follow-on change if material.
- R6 introduces **NEW** canonical specs (4 new folders under `openspec/specs/`) — but those specs already exist change-locally. Apply = archive-with-promotion; no delta-spec emission needed since the spec text is already validated change-locally.

### Q5: Capability naming for new specs from R6 promotion?

- `dynamic-tool-provider` (already exists change-locally, will be promoted)
- `tool-adapter` (already exists change-locally)
- `tool-runtime` (already exists change-locally)
- `provider-hooks` (already exists change-locally)
- `tool-cancellation-propagation`, `async-permission-deferred`, `scoped-tool-registry`, `doom-loop-proactive-detection`, `smart-compaction-agent` — from R2 promotion (5 new folders)

### Q6: Task granularity?

- Each recommendation has its own task group (## 1 through ## 6).
- Inside, sub-tasks grouped by dependency order: code → tests → archive steps.
- Use small (1-2 file) sub-tasks where possible; R3 spec-hygiene grouped into one batch task (mechanical fix).

## Design trade-offs (carried into design.md §Risks)

- **Trade-off: bundled change vs. atomic per-recommendation** → accept reduced isolation in exchange for a single verification surface. Mitigation: tasks structured so each recommendation can be done-and-verified in isolation before the next starts.
- **Trade-off: modifying architecture-audit spec twice (R3 + R4)** → already noted; resolve by sequencing R3 doc-only edit before R4 verification scenario edit.
- **Trade-off: R5 as research deliverable in same change** → keeps context intact but delays archival until research is reviewed. Mitigation: R5 marked as `<!-- research-only, not on critical path -->` in tasks.md.

## Source artifacts referenced (read-only)

- `openspec/changes/production-grade-agent-architecture/{proposal,design,tasks,brainstorm}.md`
- `openspec/changes/add-dynamic-tool-provider-system/{proposal,tasks,specs/*/spec.md}`
- `openspec/specs/architecture-audit/spec.md`
- `openspec/specs/*/spec.md` (71 files — sampled)
- Explore agent session `ses_0aa603107ffeXk4zX4iRb6wgh9` (recommendations R1-R6, ranked)

## Disagreements with explore agent

None material. Accepted all 6 ranked recommendations. Add explicit "Items Considered and Rejected" section to proposal.md to record the reasoning trail.
