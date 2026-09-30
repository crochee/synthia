# Outstanding Follow-up Resolution Log (R8/R3 path A continuation)

## R3 follow-on resolved

Resolved one of the three outstanding follow-ons documented in the v3 final summary:
the **R3 spec-hygiene** item ("fill `## Purpose` on the 5 high-impact specs").

### Change

Replaced the placeholder `## Purpose: TBD - created by archiving change <name>.`
lines on 4 of the 5 deferred specs with real Purpose prose:

| Spec | Old Purpose | New Purpose |
|---|---|---|
| `agent-bus` | TBD | `AgentBus` trait surface for inter-agent communication (register/send/broadcast/subscribe); implementations interchangeable behind the trait |
| `context-compaction` | TBD | L1-L4 compaction cascade; tiktoken-precise triggers, summary chaining via `previous_summary`, `TokenBudgetWarning` / `MustCompact` contracts |
| `agent-react-loop` | TBD | Mark legacy `ReActLoop` as `#[deprecated]`; migrate external consumers to `StreamBuilder` |
| `convergent-prompt-assembly` | TBD | Single public entry point `synthia_context::assembler::ContextAssembler`; removes private `ContextBuilder` from `synthia-agent` |

`architecture-audit/spec.md` was already filled during v3 R8 (commit `7393a7a`),
so it did not need re-touching.

### Validation

```
$ openspec validate agent-bus --strict            ✓ valid
$ openspec validate context-compaction --strict   ✓ valid
$ openspec validate agent-react-loop --strict     ✓ valid
$ openspec validate convergent-prompt-assembly --strict  ✓ valid
```

All four specs pass strict validation after the Purpose edit. Delta specs
remain intact; no Requirement or Scenario edits.

### Git tracking

These spec files live at `openspec/specs/<name>/spec.md`. Per the user's earlier
"docs local only" decision, `openspec/` is `.gitignore`d at the repository root
(line 38 of `.gitignore`). The Purpose edits therefore persist **locally only**
and are not part of the git-tracked branch `feature/synthia-session-v2`.

This is intentional. The git branch remains focused on the 10 code-or-config
commits that make up the actual rollout (R1-R9); the Purpose prose is the kind
of descriptive spec metadata the project keeps out of version control.

The 4 edited files are the `## Purpose` section only; the requirements bodies
were not touched, so any spec delta that previously existed in the archive
remains the source of truth for what changed in those specs.

## Still outstanding (not in scope of this log)

- **R1 outstanding**: `BashToolsProvider` / `MCPToolsProvider` / `SearchToolsProvider`
  + `register_defaults` deprecation + 3 caller migrations
  (`crates/synthia-cli/src/repl_core/repl/agent_message.rs:62`,
  `crates/synthia-server/src/state/app_state.rs:108`,
  `crates/synthia-agent/src/subagent/config.rs:102`).
  Recorded as deferred in `2026-07-14-adopt-explore-agent-recommendations/`.

- **R2 outstanding**: promote 5 `production-grade-agent-architecture` specs to
  canonical `openspec/specs/` + archive the source change
  (`openspec/changes/production-grade-agent-architecture/`).

- **R9 outstanding** (separate concern): port the 14 internal callers of the
  legacy `synthia-session/src/store/` shim to v2 types one-at-a-time, then
  delete the shim directory. Recorded in
  `crates/synthia-session/src/store/R9_DEFERRED.md`.

- **Pre-existing synthia-memory/cold/sqlite 5-test failure** unrelated to v3;
  baseline accepted.