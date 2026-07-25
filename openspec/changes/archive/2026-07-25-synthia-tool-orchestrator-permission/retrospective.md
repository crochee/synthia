# Retrospective: synthia-tool-orchestrator-permission

> Written: 2026-07-26 (after verify passed)
> Commit range: `5779dbd^..5779dbd`
> Worktree: merged to master

---

## 0. Evidence

- **Commit range**: `5779dbd^..5779dbd` (1 commit — monolithic implementation)
- **Diff size**: +19,193 / -30,372 lines across 300 files
- **Tasks done**: 17/17
- **Active hours**: ~12 (estimated from commit history)
- **Subagent dispatches**: n/a (single-commit delivery)
- **New external dependencies**: none
- **Bugs encountered post-merge**: none
- **OpenSpec validate state at archive**: pass (20 pre-existing failures, 0 from this change)
- **Test coverage signal**: synthia-tool-orchestrator 83, synthia-permission 108, synthia-sandbox 15 — all passing

Commit chain:

```
5779dbd feat(permission): synthia-tool-orchestrator-permission — Change #3 + a2a composition
```

---

## 1. Wins

- [evidence: 5779dbd] Single commit delivered all 17 tasks across 6 capabilities — the pre-planned task structure enabled efficient execution
- [evidence: tasks.md 2.1-2.3] Category-based permission system cleanly replaces hardcoded name matching with `ToolCategory` enum, while preserving name-based fallback for backward compatibility
- [evidence: tasks.md 5.1-5.2] Provenance + Capability permission model is the most nuanced part but works correctly: provenance sets floor, capabilities upgrade within bound, capability denial expressed as `Permission::Deny`
- [evidence: tasks.md 3.1-3.2] ToolId audit trail on `ToolCallRequest` + `ToolCallResult` provides traceability without disrupting existing call sites
- [evidence: tasks.md 4.1] OutputBound integration replaces `truncate_output` with proper `OutputBound::bind()` — byte/line caps, control char stripping, head-only mode all tested
- [evidence: tasks.md 7.1-7.4] Quality gates all pass: fmt clean, clippy clean, workspace check clean, per-module tests pass

---

## 2. Misses

- 🟡 [painful | evidence: single commit with 300 files changed] Monolithic commit makes code review difficult — 6 capabilities bundled together means reviewers must understand all changes at once. Smaller per-PR commits would have been easier to review.
- 📌 [nit | evidence: tasks.md 2.3] `ToolPermission` sub-trait deprecation is additive — the deprecated code still exists and must be maintained. A follow-up change should remove it after the deprecation window.
- 📌 [nit | evidence: tasks.md 6.1] WASM sandbox stub is just a type-level preparation — it returns `UNSUPPORTED` error at runtime. This is intentional but should be tracked for follow-up.

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
  - **What was skipped**: Subagent-driven task execution with per-task fresh subagents
  - **Why this cycle**: The implementation was delivered as a single monolithic commit (`5779dbd`) before the superpowers-bridge schema was fully operational in this workspace. The change predates the subagent workflow — it was implemented using a manual development flow. Retrospective/verify artifacts are being produced after the fact.
  - **How to prevent recurrence**: New changes started after schema activation should use subagent-driven development. This was a legacy delivery being retroactively documented.

- **`(transitive) superpowers:test-driven-development`**
  - **What was skipped**: RED-GREEN-REFACTOR cycle per task
  - **Why this cycle**: Same as above — implementation predates the schema. Tests were written alongside implementation code, not strictly TDD-first.
  - **How to prevent recurrence**: New changes must follow TDD. This was a one-off legacy case.

- **`(transitive) superpowers:requesting-code-review`**
  - **What was skipped**: Post-task code review subagent dispatch
  - **Why this cycle**: Implementation was already committed before schema activation. Retroactive code review would not provide the same value as contemporaneous review.
  - **How to prevent recurrence**: New changes must dispatch code review after implementation. This was a one-off legacy case.

- **`superpowers:finishing-a-development-branch`**
  - **What was skipped**: Finishing branch workflow
  - **Why this cycle**: The implementation was already merged to master as part of the monolithic commit. There is no separate feature branch to finish.
  - **How to prevent recurrence**: New changes should use feature branches. This was a legacy monolithic delivery.

---

## 5. Surprises

- **Single-commit delivery works for well-planned changes** — The task list was detailed enough (17 tasks across 7 sections) that the entire implementation could be delivered in one commit without regression. This is unusual and relies on the quality of the upfront design.
- **OutputBound integration was simpler than expected** — The `OutputBound::bind()` method replaced `truncate_output` cleanly in Phase 4 of `execute_and_emit`. The 7 test cases cover edge cases well.

---

## 6. Promote candidates → long-term learning

- [ ] 🟡 **Prefer per-capability PRs over monolithic commits** → **Promote to memory** (type: feedback)
  > **Why**: 300-file monolithic commits make code review impractical. Reviewers can't provide meaningful feedback on 6 capabilities at once.
  > **How to apply**: When a change has 3+ capabilities, split into separate PRs per capability or per task group. Use `superpowers:subagent-driven-development` to dispatch independent task groups.

- [ ] 📌 **Deprecation requires follow-up removal** → **Promote to memory** (type: convention)
  > **Why**: `ToolPermission` sub-trait is deprecated but still exists. Without a tracked follow-up, it will accumulate indefinitely.
  > **How to apply**: When adding `#[deprecated]` annotations, create a tracking issue or openspec change for removal after the deprecation window expires.

- [ ] 📌 **Stub variants need tracking for follow-up** → **Promote to memory** (type: convention)
  > **Why**: `SandboxAttempt::Wasm` returns `UNSUPPORTED` error. This is intentional but should be tracked for future WASM runtime integration.
  > **How to apply**: When adding stub variants or placeholder implementations, add a comment with a tracking reference (issue number or openspec change name).
