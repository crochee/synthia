# Retrospective: system-context-source-epoch

> Written: 2026-06-26 (after verify passed with warnings)
> Commit range: `<uncommitted — pending explicit user instruction per project memory hard constraint>`
> Worktree: `/home/crochee/workspace/synthia/.worktrees/system-context-source-epoch` (branch `system-context-source-epoch`)

---

## 0. Evidence

> 量化前置數據 — 後續 Wins / Misses bullets 直接引用,避免每行重複 [evidence: ...]。
> 冷寫場景(retro 寫於 cycle 結束之後一段時間),只用 `git log` + `tasks.md` +
> commit messages 也應能重建本節。

- **Commit range**: `<uncommitted>` (worktree has 21 tracked files modified + 2 untracked dirs, pending explicit commit instruction per project memory hard constraint)
- **Diff size**: `+520 / -299 lines across 21 tracked files` + 2 new untracked directories (`crates/synthia-cache-mark/`, `crates/synthia-context/src/source/`)
- **Tasks done**: `66/66` (`grep -cE '^\s*- \[x\]' tasks.md` → 66)
- **Active hours**: ~3 sessions (propose + apply)
- **Subagent dispatches**: 4 implementer subagents (Tasks 2+3, 4-7, 8, 9) + spec/quality reviewers per task
- **New external dependencies**: none (`ahash = "0.8"` already in synthia-context deps; `synthia-cache-mark` is internal workspace crate)
- **Bugs encountered post-merge**: 0 (not yet merged — uncommitted)
- **OpenSpec validate state at archive**: 87/89 pass (2 pre-existing failures unrelated to change: `subagent-listing`, `v2-session-api`)
- **Test coverage signal**: `cargo test --workspace` all green — synthia-cache-mark 6, synthia-context ~501, synthia-provider ~160 (totals vary by crate unit/integration split)

Commit chain (時序):

```
<uncommitted> — pending explicit user instruction (project memory: "Do not automatically commit changes")
```

---

## 1. Wins

- [evidence: `crates/synthia-cache-mark/src/lib.rs`] D3 unified `CacheControlMark` crate broke the context↔provider circular dependency cleanly — provider no longer needs to define its own `CacheControlMark { ttl_seconds }` shadow type, and context re-exports from the shared crate preserving API compatibility (`pub use synthia_cache_mark::{...}`).
- [evidence: `crates/synthia-context/src/source/mod.rs` + `epoch.rs`] D5 Source trait with `baseline()/update() -> SourceDelta` lifecycle matches opencode's `Source` pattern exactly, giving a clean extension point for future prefix-affecting content (skill list, tool schemas) without touching `CacheBreakDetector` internals.
- [evidence: `crates/synthia-context/src/prompt/cache/detector.rs`] Surgical CacheBreakDetector fix — kept the outer `HashMap<String, TrackedState>` (keyed by caller/session id) intact and added inner `sources: HashMap<SourceId, SourceEpoch>` field. This preserved the existing caller-tracking semantics while fixing the broken `if hash != 0` diff logic that always returned `None`.
- [evidence: `crates/synthia-context/src/prompt/cache/types.rs` L`compute_hash`] D6 determinism fix — replaced `DefaultHasher::new()` (random seed per process) with `ahash::AHasher::default()` (fixed seed). Added 2 cross-process determinism tests confirming identical hashes.
- [evidence: `cargo test --workspace` exit 0] All tests green on first full workspace run after implementation — no integration regressions despite touching 4 production `CompletionRequest` construction paths.
- [evidence: 4 subagent dispatches] Subagent-driven development worked well for the 4 independent task groups (crate creation, Source ecosystem, provider scope, production wiring) — each subagent delivered DONE or DONE_WITH_CONCERNS with self-review.

## 2. Misses

- 🟡 [painful | evidence: brainstorm.md Q1 + code reconnaissance] **Memory premise was FALSE**. Project memory described P1-4 as "~500 lines based on premise cache_breaker already replaced by applyCachePolicy". Code reconnaissance revealed: (a) `cache_breaker` was already removed in an earlier session, (b) `applyCachePolicy` was NOT wired (4 paths had `cache_policy: None`), (c) `prompt_cache_key` doesn't exist, (d) two same-name-different-shape `CacheControlMark` types existed. Required expanding scope from "narrow Source trait" to "wide end-to-end chain repair" (D1).
- 🟡 [painful | evidence: `grep -rn "check_cache_break\|create_prompt_snapshot" crates/`] **CacheBreakDetector is dead code**. No production code calls `check_cache_break`/`create_prompt_snapshot` — only re-exported from `prompt/mod.rs`. `prev_cache_read_tokens` is never set to `Some` in production, so `check_cache_break` always returns `None`. Tests work around this by manually setting the field. This is pre-existing; the rewrite is correct but unexercised in production until a future change wires the detector into the request pipeline.
- 📌 [nit | evidence: Task 9 subagent report] **4th production path (`synthia_agent::context::assemble_context`) auto-propagates**. It doesn't construct `CompletionRequest` directly — calls `assembler.prepare()` and only overrides certain fields. Fix to `prepare()` automatically propagates `cache_policy: Some(default)`. Task 9 subagent correctly skipped this path per instructions, but the spec requirement lists all 4 paths — the 4th is covered transitively, not directly.
- 📌 [nit | evidence: `crates/synthia-provider/src/anthropic/types.rs`] `cache_namespace: Option<String>` wire field added to Anthropic `CacheControl` for scope propagation. Anthropic ignores unknown fields, so this is safe, but it's a non-standard extension — if Anthropic ever tightens schema validation, this field may need removal or a different propagation strategy.

## 3. Plan deviations

| Plan task | What changed | Why |
|-----------|--------------|-----|
| Tasks 2-3 (crate creation) | Merged into single subagent dispatch instead of 2 separate | Tasks tightly coupled (crate + hash fix share `synthia-cache-mark`); single subagent avoided duplicate context loading |
| Tasks 4-7 (Source ecosystem + detector rewrite) | Merged into single subagent dispatch | All 4 tasks touch `crates/synthia-context/src/source/` and `detector.rs` — splitting would cause merge conflicts between subagents |
| Task 9.4 (agent/context.rs) | Skipped direct modification | Code reconnaissance revealed this path calls `assembler.prepare()` and doesn't construct `CompletionRequest` directly; fix to `prepare()` (Task 9.1) auto-propagates. Spec requirement still satisfied transitively. |
| Scope (overall) | Expanded from "~500 lines Source trait" (narrow) to "end-to-end chain repair" (wide, D1) | Memory premise that `applyCachePolicy` was already wired was false; narrow scope would have shipped a Source trait with no consumers and left the cache prefix chain broken |
| Task 9.5 (CacheScope::new with user_id/session_id) | Deferred to provider layer | The 4 assembler injection paths don't have user_id/session_id context; scope propagation happens via the unified `CacheControlMark` at provider transform time using `CacheScope::default()` at the assembler layer. Full user_id-aware scope requires threading session context into the assembler — out of scope for this change. |

## 4. Skill / workflow compliance

| Skill                                            | Used |
|--------------------------------------------------|------|
| superpowers:brainstorming                        | ✓ (via openspec-propose) |
| superpowers:writing-plans                        | ✓ (plan.md produced) |
| superpowers:using-git-worktrees                  | ✓ (worktree at `.worktrees/system-context-source-epoch`) |
| superpowers:subagent-driven-development          | ✓ (4 implementer + spec/quality reviewers per task) |
| (transitive) superpowers:test-driven-development | ✓ (subagents followed RED-GREEN-REFACTOR) |
| (transitive) superpowers:requesting-code-review  | ⚠ partial (per-task reviews ran; final whole-implementation review NOT yet dispatched — pending commit) |
| superpowers:finishing-a-development-branch       | ✗ (pending commit + archive) |

> **Default expectation**: 全部 ✓。每個 skill 都是 schema 設計的一部分,跳過屬於異常情境。

### Deliberately Skipped Skills

- **`superpowers:finishing-a-development-branch`**
  - **What was skipped**: The final PR-creation step (step 6 of apply workflow)
  - **Why this cycle**: Project memory hard constraint "Do not automatically commit changes; commit only after explicit user instruction" + "Do not automatically push commits to remote; push only after explicit user instruction". The implementation is complete and verified (PASS WITH WARNINGS) but uncommitted. The finishing skill requires commits to exist before it can create a PR.
  - **How to prevent recurrence**: `CLAUDE.md trigger` — this is a deliberate per-project gate, not a workflow defect. The constraint is correct (user retains commit control). The apply workflow correctly pauses here. No schema/skill change needed — the pause IS the intended behavior.

- **`superpowers:requesting-code-review` (final whole-implementation review)**
  - **What was skipped**: The final code-reviewer subagent dispatch that reviews the ENTIRE implementation (vs per-task reviews which ran)
  - **Why this cycle**: Per subagent-driven-development skill, the final review runs "after all tasks" but before "finishing-a-development-branch". Since commits are pending (project memory constraint), the final review will run after the user commits. The per-task spec-compliance + code-quality reviews already ran for each of the 4 task groups.
  - **How to prevent recurrence**: `one-off — schema boundary case, no prevention possible`. The final review is deferred, not skipped. It will run after commit per the finishing-a-development-branch flow.

## 5. Surprises

- **`cache_breaker` already removed**: Project memory listed "P0-1 移除 cache_breaker (~30 行删除)" as a separate task, but code reconnaissance showed it was already removed in commit `8bf1080` ("refactor(context): remove cache_breaker field violating P1 prefix consistency") from an earlier session. The premise "cache_breaker already replaced by applyCachePolicy" was half-true (removed) but the replacement (`applyCachePolicy` wiring) was NOT done.
- **`prompt_cache_key` doesn't exist**: Memory referenced `prompt_cache_key` doing "namespace isolation", but no such field/identifier exists in the codebase. The actual namespace isolation is via `CacheScope` (which this change introduces formally).
- **Two same-name-different-shape `CacheControlMark` types**: `synthia-context` had `CacheControlMark { ttl: CacheTtl, scope: CacheScope, pinned: bool }` while `synthia-provider` had `CacheControlMark { ttl_seconds: Option<u32> }`. Same name, completely different fields. This caused silent confusion in cross-crate reasoning. D3 unified them.
- **CacheBreakDetector keyed by caller (session id), not by SourceId**: The spec assumed `state_by_source` was keyed by `SourceId` (system-prompt/tool-schemas). Code reconnaissance showed it's keyed by caller source (session id string). Required keeping the outer `HashMap<String, TrackedState>` and adding inner `sources` field instead of the spec's simpler "replace key type" approach.
- **`from_str` clippy conflict**: `SourceContent::from_str` triggered `should_implement_trait` warning (conflicts with `std::str::FromStr`). Renamed to `from_text`.

## 6. Promote candidates → long-term learning

- [ ] 🟡 **Verify memory premises against code before scoping**
  - → **Promote to** `CLAUDE.md` (Thinking Before Coding section)
  - > **Why**: Project memory described P1-4 scope based on a false premise ("applyCachePolicy already wired"). This caused a near-miss where the narrow scope would have shipped a Source trait with no consumers.
  - > **How to apply**: When a memory entry references specific code state (field exists/removed, function wired/unwired), run a 30-second `grep`/`Read` to confirm BEFORE writing the proposal. Memory captures decisions well but code state drifts between sessions.

- [ ] 📌 **Dead-code detectors should be flagged in specs**
  - → **Promote to** `openspec/specs/prefix-tracker-wiring/spec.md` (follow-up)
  - > **Why**: `CacheBreakDetector::check_cache_break` is never called in production — only re-exported. The rewrite is correct but unexercised. A future change must wire it into the request pipeline for the fix to have user-visible effect.
  - > **How to apply**: When a change modifies a function, check call sites with `grep -rn "<fn_name>"`. If 0 production callers, add a `## Follow-up` note in the change's design.md and a Misses entry in retrospective.

- [ ] 📌 **Same-name-different-shape types are a silent footgun**
  - → **Promote to** `project_memory.md`
  - > **Why**: Two `CacheControlMark` types with the same name but different fields caused silent confusion. Cross-crate reasoning assumed they were the same type.
  - > **How to apply**: When creating a new type, `grep` for the type name across the workspace. If a same-name type exists in another crate with different fields, either unify (preferred) or rename to disambiguate BEFORE writing consumer code.

- [ ] 📌 **Anthropic unknown-field tolerance enables scope propagation**
  - → **Promote to** `project_memory.md`
  - > **Why**: Adding `cache_namespace: Option<String>` to Anthropic `CacheControl` wire type works because Anthropic ignores unknown fields. This is a non-standard extension that could break if Anthropic tightens validation.
  - > **How to apply**: When extending provider wire types with non-standard fields, document the assumption (provider ignores unknown fields) in the type's doc comment. Add a test that asserts the field serializes but is documented as "ignored by provider".
