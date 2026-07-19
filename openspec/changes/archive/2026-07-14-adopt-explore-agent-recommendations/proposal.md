# Proposal: adopt-explore-agent-recommendations

## Why

Synthia has mature in-flight work and shipped capabilities that the OpenSpec tooling still treats as "unaddressed." `production-grade-agent-architecture` shipped all 5 P0/P1 capabilities to master (tool cancellation, async permission, scoped registry, doom-loop detection, smart compaction) but the change was never archived and specs were never promoted to `openspec/specs/`. `add-dynamic-tool-provider-system` shipped Phase 1 only; 3 of 4 `ToolProvider` implementations (`BashToolsProvider`, `MCPToolsProvider`, `SearchToolsProvider`) and the `register_defaults` deprecation remain. 71 archived specs carry `## Purpose: TBD` boilerplate that blocks `openspec validate --all`. `architecture-audit` spec's 3 requirements are satisfied in code but never verified. Closing these gaps now unblocks the verify pipeline, removes stale "0/N tasks" noise, and reduces future change-setup cost.

## What Changes

**R1 — Complete `add-dynamic-tool-provider-system` Phase 2**
- From: Only `FileToolsProvider` exists; bash / MCP / search tools still flow through the deprecated `ToolRegistry::register_defaults()` path
- To: `BashToolsProvider`, `MCPToolsProvider`, `SearchToolsProvider` implemented; `register_defaults()` deprecated; CLI and server construct `ExtensionManager` from providers
- Reason: Single source of truth for tool registration, MCP extensibility, no static registration at startup
- Impact: Non-breaking — `register_defaults()` retained behind `#[deprecated]` for one minor version

**R2 — Close out `production-grade-agent-architecture`**
- From: All 5 capabilities shipped in code (8 merge commits visible on master), but change-local specs sit at `openspec/changes/.../specs/` and tasks.md shows 0/83
- To: 5 specs promoted to canonical `openspec/specs/{tool-cancellation-propagation,async-permission-deferred,scoped-tool-registry,doom-loop-proactive-detection,smart-compaction-agent}/spec.md`, tasks.md marked `[x]`, change archived
- Reason: `openspec validate --all` and the tooling's change-list rely on canonical specs
- Impact: Non-breaking — archival and bookkeeping only

**R3 — Spec hygiene sweep**
- From: 71 specs in `openspec/specs/*/spec.md` carry `## Purpose\nTBD - created by archiving change X. Update Purpose after archive.`
- To: `## Purpose` section added to the 5 highest-impact specs (`architecture-audit`, `agent-bus`, `context-compaction`, `agent-react-loop`, `convergent-prompt-assembly`)
- Reason: Spec quality and downstream tooling; mechanical fix
- Impact: Non-breaking — documentation only

**R4 — Verify & archive `architecture-audit`**
- From: 3 Requirements in `openspec/specs/architecture-audit/spec.md` are code-satisfied but never formally verified or archived
- To: Each Requirement gets a `#### Scenario:` marked `VERIFIED` (or a new scenario confirming current behavior); spec archived
- Reason: Closes spec debt so it doesn't appear in future search results as "incomplete audit"
- Impact: Non-breaking — verification + archive

**R5 — Research note on 4 P2 capability gaps**
- From: `production-grade-agent-architecture/brainstorm.md` identifies 9 capability gaps; only 5 (P0+P1) selected. The 4 P2 items are deferred but never enumerated in writing.
- To: A Markdown feasibility note at `openspec/changes/adopt-explore-agent-recommendations/research/p2-gap-feasibility.md` comparing Synthia's current state to OpenCode/Codex/pi-mono on each of: Effect-rs framework adoption, full event sourcing, WebSocket transport resilience, Fiber-based automatic cancellation
- Reason: Informs future prioritization; non-binding; not on critical path
- Impact: Non-breaking — research deliverable only

**R6 — Promote `add-dynamic-tool-provider-system` specs to canonical**
- From: After R1 lands, 4 specs remain change-local at `openspec/changes/add-dynamic-tool-provider-system/specs/`
- To: 4 specs promoted to canonical `openspec/specs/{dynamic-tool-provider,tool-adapter,tool-runtime,provider-hooks}/spec.md`; change archived
- Reason: Mirrors R2 archival pattern
- Impact: Non-breaking — follows R1; no separate delta-spec emission since change-local specs are already validated

## Capabilities

### New Capabilities

- `tool-cancellation-propagation`: Promotion of existing change-local spec from `production-grade-agent-architecture` to canonical location (R2)
- `async-permission-deferred`: Promotion of existing change-local spec (R2)
- `scoped-tool-registry`: Promotion of existing change-local spec (R2)
- `doom-loop-proactive-detection`: Promotion of existing change-local spec (R2)
- `smart-compaction-agent`: Promotion of existing change-local spec (R2)
- `dynamic-tool-provider`: Promotion of existing change-local spec to canonical location (R6)
- `tool-adapter`: Promotion of existing change-local spec (R6)
- `tool-runtime`: Promotion of existing change-local spec (R6)
- `provider-hooks`: Promotion of existing change-local spec (R6)

### Modified Capabilities

- `architecture-audit` (existing spec, `openspec/specs/architecture-audit/spec.md`):
  - **R3** adds a `## Purpose` section with a 1-2 sentence summary
  - **R4** adds VERIFIED-marked scenarios to each of the 3 existing Requirements

## Impact

**Affected crates (R1 only):** `synthia-agent`, `synthia-mcp`, `synthia-tool-bash`, `synthia-cli`, `synthia-server`. New files: `crates/synthia-agent/src/tools/providers/bash_tools_provider.rs`, `crates/synthia-agent/src/tools/providers/mcp_tools_provider.rs`, `crates/synthia-agent/src/tools/providers/search_tools_provider.rs`. Modified files: `synthia-cli/src/repl_core/repl/agent_message.rs`, `synthia-server/src/state/app_state.rs`, `synthia-agent/src/subagent/config.rs`, `synthia-tool/src/registry.rs` (deprecation annotations).

**Affected specs (R2-R6):** 14 spec folders under `openspec/specs/` gain new or promoted files. 1 spec (`architecture-audit`) gains a `## Purpose` section + 3 verification scenarios.

**No breaking changes** to public APIs. `ToolRegistry::register_defaults` is deprecated but retained.
