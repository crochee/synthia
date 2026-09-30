# Retrospective: agents-md-hierarchical-discovery

> Written: 2026-06-13 (after merge to master)
> Base commit: `1d59c49`
> Final commit: `628a8e3`

---

## 0. Evidence

- **Commit range**: `1d59c49..628a8e3` (5 feature commits, no merge commit — applied directly to master)
- **Diff size**: ~990 lines added across 6 files
- **Tasks done**: 9/9 plan tasks (scaffold, walk+merge, size limits, PromptSection, builder wiring, identity cleanup, agent config, e2e, quality gates)
- **Subagent dispatches**: 0 (single-agent, surgical scope)
- **New external dependencies**: none
- **Bugs encountered post-merge**: 0
- **OpenSpec validate state at archive**: 1/1 pass

---

## 1. Wins

- [evidence: `walk_ancestors` + canonical-path dedup] **Symlink cycle protection by design**: HashSet of canonical paths naturally handles `loop_dir/inside → loop_dir` symlink without special casing. Covered by `test_walk_handles_circular_symlink`.
- [evidence: `merge_within_limit` iterates farthest→closest, naturally favoring the closest file] **Closest-file-wins is emergent, not enforced**: by processing the closest file LAST, the budget check happens first on the closest (so it always fits when there's room). The marker is appended to the second-to-last file. No special "always include closest" branch needed.
- [evidence: `agents_md_config()` bridge method] **Agent↔Context seam is a single 12-line method**: `AgentConfig::agents_md_config()` produces a `synthia_context::prompt::sections::agents_md::AgentsMdConfig` with empty-filenames fallback. No shared trait abstraction needed; just a pure function.
- [evidence: `WORKSPACE_FILES` is now `pub const`] **IdentitySection exposes its workspace list as a public constant**: lets `agents_md_discovery::test_workspace_files_excludes_agents_md` enforce the "AGENTS.md is owned by AgentsMdSection" invariant at compile time. No need for integration test only.
- [evidence: `..Default::default()` at every call site] **Adding 2 fields to `AgentConfig` required 0 call-site edits**: all 6 existing struct-literal sites already use `..Default::default()`. New fields flow through cleanly.
- [evidence: serde `default = "..."` functions] **Backward-compatible TOML**: `test_agent_config_serde_backward_compat_no_agents_md_fields` proves older operator configs without `agents_md_*` keys still deserialize. No migration needed.

## 2. Misses

- 📌 [evidence: agents_md.rs:359 used `assert_eq!(x, false)` initially] **Bool literal clippy trap**: wrote 2 `assert_eq!(x, true/false)` patterns before clippy flagged them. Switched to `assert!` / `assert!(!x)`. Lesson: when in doubt, use `assert!` for booleans.
- 📌 [evidence: required switching test_workspace_files_excludes_agents_md to access `super::identity::WORKSPACE_FILES`] **WORKSPACE_FILES had to be `pub`**: was originally private; promoting to `pub const` was a minor exposure increase. The alternative (duplicating the constant list in the test) was worse. Worth documenting if `IdentitySection` later wants to hide it.
- 📌 [evidence: `PromptBuilder::sections` is private, blocking one test] **Test had to use `state.get("agents_md", SessionCached)` to verify caching level**: couldn't directly read `builder.sections[idx].caching()`. Functional, but less direct than the unit test would be. Could expose `section_caching(name: &str) -> Option<SectionCaching>` in a follow-up.

## 3. Plan deviations

| Plan task | What changed | Why |
|-----------|--------------|-----|
| Task 1 | Combined Task 1-4 (scaffold + walk + size limits + PromptSection impl) into a single 638-line file and a single commit | Logical cohesion: all four touch the same module; no value in splitting into 4 PRs. Plan's separation was for "one PR per task" worktree workflow; this change is small enough to commit atomically. |
| Task 5 | `default_with_sections` and `build_for_name` both updated to register the section (plan called for both) | Plan specified both; no deviation. |
| Task 6 | `WORKSPACE_FILES` promoted from `const` to `pub const` | Needed by regression test `test_workspace_files_excludes_agents_md` |
| Task 7 | Added `agents_md_config()` bridge method (not in plan) | Plan said "wire AgentsMdSection into PromptBuilder" with new fields. The bridge method was needed so the agent can hand config to a `with_config` constructor at runtime; plan implicitly assumed it but didn't name it. |
| Task 8 | E2E test file uses `state.get` to verify caching level | See Misses #3. Test still covers SessionCached semantics via modify-file-across-resolve cycle. |

## 4. Skill / workflow compliance

| Skill | Used |
|-------|------|
| superpowers:brainstorming | ✓ (brainstorm.md captures 12 design Q&As) |
| superpowers:writing-plans | ✓ (plan.md with 9 task groups, each with TDD-style micro-steps) |
| superpowers:verification-before-completion | ✓ (verify.md produced, all gates checked) |
| superpowers:test-driven-development | ✓ (tests written before/with each module, 31 total) |
| TDD per module | ✓ (21 unit tests in agents_md.rs, 6 e2e tests, 4 config tests) |

## 5. Open follow-ups

- The `PromptBuilder` could be enhanced to accept an `AgentsMdConfig` parameter in `default_with_sections()` and `build_for_name()`. Currently, the runtime code that builds the prompt must call `with_config` on a custom `AgentsMdSection` and `add_section` it manually. This works but is verbose. A `build_with_agent_config(agent: &AgentConfig)` overload would close the loop.
- `WORKSPACE_FILES` being `pub` invites external mutation. If we want to keep it private while still testable, expose `pub fn has_workspace_files` plus a test-only `pub(crate) const WORKSPACE_FILES_FOR_TEST`.
- Consider whether the bridge method `AgentConfig::agents_md_config()` should live in `synthia_context` instead — pulling config into a section constructor is a generalizable pattern.

## 6. Decision: archive this change

The change is small, self-contained, and ships with full test coverage. The 5 commits are clean and rollback-able. The OpenSpec change artifacts (`brainstorm`, `design`, `proposal`, `plan`, `tasks`, `specs`, `verify`, this retrospective) trace the full design chain.

**Action**: archive to `openspec/changes/archive/2026-06-13-agents-md-hierarchical-discovery/` and sync delta specs to `openspec/specs/`.
