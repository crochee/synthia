## Context

The codebase has build errors. Synthia-context builds after fixing deps. Synthia-agent has 68+ errors.

## Goals / Non-Goals

**Goals:**
1. Make `cargo build` pass
2. Fix module naming conflicts
3. Add missing dependencies

**Non-Goals:**
- Not implementing new features
- Not changing API behavior
- Not fixing all warnings

## Decisions

### D1: Keep mod.rs over standalone file

- **選擇**：For conflicting modules (prompt, agent, types), keep the `mod.rs` file and remove or rename the standalone `.rs` file
- **理由**：`mod.rs` is the traditional Rust module file, better supported
- **已考慮 alternative**：Keep standalone files, rename mod.rs directories → More disruptive to existing imports

### D2: synthia-context deps fix order

- **選擇**：Add deps in order they appear in errors
- **理由**：Systematic approach
- **已考慮 alternative**：Add all at once → Harder to identify which fix helped

## Risks / Trade-offs

[Risk] Removing agent.rs/types.rs might break existing code paths → Mitigation: Keep exports in mod.rs, update lib.rs re-exports

[Trade-off] Manual fix vs automated → Accept manual to understand structure

## Migration Plan

1. Fix synthia-context deps (done in worktree attempt)
2. Analyze synthia-agent errors
3. Fix module conflicts
4. Verify build

## Open Questions

1. Are the synthia-agent errors from incomplete implementation or just module conflicts?
2. Should we attempt to fix incomplete code or mark it as TODO?