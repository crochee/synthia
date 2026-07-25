# Retrospective: synthia-agent-composition-a2a

> Written: 2026-07-26 (after verify passed)
> Commit range: `aabae11^..5779dbd`
> Worktree: merged to master

---

## 0. Evidence

- **Commit range**: `aabae11..5779dbd` (2 primary commits + follow-ups)
- **Diff size**: +26,291 / -31,955 lines across 395 files (combined)
- **Tasks done**: 64/64
- **Active hours**: ~24 (estimated)
- **Subagent dispatches**: n/a (pre-schema delivery)
- **New external dependencies**: `a2a-lf`, `a2a-client-lf`, `a2a-server-lf` (A2A protocol libraries)
- **Bugs encountered post-merge**: none
- **OpenSpec validate state at archive**: pass (20 pre-existing failures, 0 from this change)
- **Test coverage signal**: synthia-a2a 34, synthia-agent passing, synthia-tool passing

Commit chain:

```
aabae11 feat(synthia-fullstack-integration): full-stack A2A, Neon Terminal UI, management, E2E, deployment
5779dbd feat(permission): synthia-tool-orchestrator-permission — Change #3 + a2a composition
```

---

## 1. Wins

- [evidence: tasks.md §1] AgentHandle/AgentSession separation cleanly decouples stateless capability from per-session state — the `AgentHandle` can be shared across N sessions
- [evidence: tasks.md §2] `agent_as_tool()` pure function is the single composition primitive — GeneratorVerifier, Workflow, and Transfer all compose from it naturally
- [evidence: tasks.md §3] synthia-a2a crate provides standard A2A protocol communication — both in-process (AgentHandle) and remote (HTTP/gRPC) are unified
- [evidence: tasks.md §4] SendMessage/SendMessageStream tools cover both synchronous and streaming A2A patterns — consistent API for local and remote agents
- [evidence: tasks.md §5] Multi-agent pattern layer (GeneratorVerifier, Workflow, Transfer) all compose from agent_as_tool() — no special-purpose primitives needed
- [evidence: synthia-a2a 34 tests] Good test coverage for the A2A transport layer

---

## 2. Misses

- 🟡 [painful | evidence: monolithic delivery in 2 commits] 64 tasks across 5 sections delivered in 2 large commits — code review is impractical at this scale. Per-section or per-capability PRs would have been more reviewable.
- 🟡 [painful | evidence: AgentRunConfig simplification (task 1.5)] Reducing AgentRunConfig fields caused cascading changes across the codebase — every call site that constructed an AgentRunConfig needed updating. This should have been split into a separate change.
- 📌 [nit | evidence: `type AgentInstance = AgentHandle` alias] The type alias preserves backward compatibility but adds cognitive load — consumers must know two names for the same thing. Should be removed in Phase 6.

---

## 3. Plan deviations

| Plan task | What changed | Why |
|-----------|--------------|-----|
| None | All tasks implemented as planned | — |

No deviations from the plan.

---

## 4. Skill / workflow compliance

| Skill                                            | Used |
|--------------------------------------------------|------|
| superpowers:brainstorming                        | ✓    |
| superpowers:writing-plans                        | ✓    |
| superpowers:using-git-worktrees                  | ✓    |
| superpowers:subagent-driven-development          | ✗    |
| (transitive) superpowers:test-driven-development | ✗    |
| (transitive) superpowers:requesting-code-review  | ✗    |
| superpowers:finishing-a-development-branch       | ✗    |

### Deliberately Skipped Skills

- **`superpowers:subagent-driven-development`**
  - **What was skipped**: Subagent-driven task execution
  - **Why this cycle**: Implementation was delivered before the superpowers-bridge schema was operational in this workspace. The change predates the subagent workflow — retroactive documentation.
  - **How to prevent recurrence**: New changes started after schema activation should use subagent-driven development. This was a one-off legacy case.

- **`(transitive) superpowers:test-driven-development`**
  - **What was skipped**: RED-GREEN-REFACTOR per task
  - **Why this cycle**: Same as above — implementation predates schema activation.
  - **How to prevent recurrence**: New changes must follow TDD. One-off legacy case.

- **`(transitive) superpowers:requesting-code-review`**
  - **What was skipped**: Post-task code review
  - **Why this cycle**: Implementation already committed before schema activation.
  - **How to prevent recurrence**: New changes must dispatch code review. One-off legacy case.

- **`superpowers:finishing-a-development-branch`**
  - **What was skipped**: Finishing branch workflow
  - **Why this cycle**: Already merged to master as monolithic delivery.
  - **How to prevent recurrence**: New changes should use feature branches. One-off legacy case.

---

## 5. Surprises

- **agent_as_tool() composition works better than expected** — GeneratorVerifier, Workflow, and Transfer all compose naturally from a single primitive without special-casing. This validates the "composition over inheritance" design principle.
- **A2A protocol integration was smoother than expected** — The `a2a-lf` libraries provided clean client/server abstractions that mapped directly to the AgentHandle/AgentSession model.
- **AgentRunConfig simplification had wider blast radius than expected** — Touching AgentRunConfig rippled through 15+ call sites across 5 crates.

---

## 6. Promote candidates → long-term learning

- [ ] 🟡 **Prefer per-capability PRs over monolithic delivery** → **Promote to memory** (type: feedback)
  > **Why**: 64-task monolithic commits make code review impractical. Reviewers can't provide meaningful feedback on 5 sections at once.
  > **How to apply**: When a change has 3+ sections, split into separate PRs per section. Use `superpowers:subagent-driven-development` to dispatch independent task groups.

- [ ] 🟡 **AgentRunConfig changes are high-blast-radius** → **Promote to memory** (type: feedback)
  > **Why**: Simplifying AgentRunConfig fields touched 15+ call sites across 5 crates. This should be a standalone change, not bundled with other work.
  > **How to apply**: When modifying AgentRunConfig, isolate the change and get it reviewed/merged before building on top of it.

- [ ] 📌 **Type aliases for backward compatibility need removal tracking** → **Promote to memory** (type: convention)
  > **Why**: `type AgentInstance = AgentHandle` preserves compatibility but adds cognitive load. Without tracking, it persists indefinitely.
  > **How to apply**: When adding type aliases for backward compatibility, create a tracking issue for removal after the migration period.
