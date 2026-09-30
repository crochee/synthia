# Verify: apply-patch-tool

> Written: 2026-06-13 (after merge to master)
> Branch: master (merged via `1509339`)
> Base commit: `24c7b79` (last non-apply-patch commit before merge)
> Final commit: `1509339` (merge of `feat/apply-patch-tool`)

---

## 0. Evidence

- **Commits**: 2 feature commits + 1 merge
  - `494b864` `feat(tool): add apply_patch builtin tool for V4A sequential multi-file edits`
  - `8404ab7` `test(tool): port all 22 codex apply-patch scenarios`
  - `1509339` (merge) `Merge branch 'feat/apply-patch-tool': V4A apply_patch tool with 22 codex scenarios`
- **Files changed**: 5 (impl) + 1 (test) + 22 (scenario fixtures)
  - `crates/synthia-tool/src/builtin/mod.rs` (export `apply_patch` + `v4a` modules)
  - `crates/synthia-tool/src/builtin/apply_patch.rs` (new, ~470 lines, incl. sequential apply + AppliedFailure + Move runtime reject)
  - `crates/synthia-tool/src/builtin/v4a.rs` (new, ~260 lines, V4A parser + `Hunk` with `Vec<HunkLine>` source-order preservation)
  - `crates/synthia-tool/src/registry/registration.rs` (register `ApplyPatchTool` in `register_defaults()`)
  - `crates/synthia-tool/tests/codex_scenarios.rs` (new, fixture-based runner parameterized over 22 scenarios)
  - `crates/synthia-tool/tests/fixtures/codex/001_add_file/` ... `022_update_file_end_of_file_marker/` (22 codex portable scenario fixtures, each with `input/` + `patch.txt` + `expected/`)
- **Test delta**: +24 codex_scenarios tests
  - `synthia-tool` lib: 67 tests
  - `synthia-tool/tests/codex_scenarios.rs`: 24 tests (22 codex scenarios + 1 discovery test + 1 fixture sanity)
  - `synthia-tool/tests/registry_test.rs`: 5 tests
  - **Total `synthia-tool`: 96 tests, 0 failed**
  - `synthia-agent` lib: 496 tests, 0 failed (regression check after registry change)
- **Subagent dispatches**: 0 (single-agent, surgical scope, well-bounded by codex 22-scenario test contract)
- **New external dependencies**: none (V4A parsing is pure text, standard library sufficient)
- **Bugs encountered post-merge**: 0 (verified via re-run of `cargo test -p synthia-tool`)
- **OpenSpec validate state at archive**: pending (this commit)

---

## 1. Spec Compliance

| Requirement (from `specs/apply-patch-tool/spec.md`) | Status |
|-------------|--------|
| V4A Patch Parsing — `Begin/End Patch` markers, 4 op headers | ✅ `v4a::parse_v4a` (state machine, 260 lines) |
| Hunk-Level Diff Application — context/insertion/deletion, `*** End of File` | ✅ `Hunk { lines: Vec<HunkLine> }` + `apply_hunks` |
| Sequential Multi-File Apply with Failure Reporting | ✅ `apply_one_op` sequential loop + `AppliedFailure { applied, failed }` |
| Path Safety and Permission Reuse — `check_path_safety` + `requires_permission() -> true` | ✅ reuses `synthia_context::paths::check_path_safety` + inherits `write` policy |
| Tool Registration and Concurrency — `register_defaults()` + `is_concurrency_safe() -> false` | ✅ `ApplyPatchTool` registered alongside `MultiEditTool` |

---

## 2. Verification Results

| Check | Result |
|-------|--------|
| `cargo test -p synthia-tool` | 96 passed; 0 failed (67 lib + 24 codex_scenarios + 5 registry) |
| `cargo test -p synthia-agent --lib` | 496 passed; 0 failed (regression check) |
| `cargo +nightly fmt --all` | no diff (pre-existing nightly/stable import-layout drift only) |
| `cargo clippy --all-targets --all-features --tests -p synthia-tool` | 0 NEW warnings (21 pre-existing workspace warnings unchanged) |
| `openspec validate apply-patch-tool --strict` | pending (this commit) |

---

## 3. Cross-Crate Compatibility

- **`ToolRegistry`**: new tool name `apply_patch` appended to the list of 8 default tools. Backward-compatible — old LLM tool calls (`read`/`write`/`multi_edit`/etc.) unaffected.
- **`ApplyPatchTool` struct**: `enable_move: bool` field defaults to `false` via `#[derive(Default)]`. Production-disabled Move ops yield `ToolFailure("apply_patch moves are not supported yet")`. Test runner uses `ApplyPatchTool { enable_move: true }` to cover codex scenarios 004/010.
- **No `AgentEvent` variants added** — no `sse.rs` match exhaustiveness ripple on the wire.
- **No `AgentConfig` field added** — no struct-literal call-site ripple.
- **No external API change** — `apply_patch` is opt-in via LLM tool selection.

---

## 4. Delta Spec Sync

Delta spec at `openspec/changes/archive/2026-06-13-apply-patch-tool/specs/apply-patch-tool/spec.md` uses the delta `## ADDED Requirements` format (per OpenSpec delta convention).

Cumulative spec synced to `openspec/specs/apply-patch-tool/spec.md` with:
- `## Purpose` section
- `## Requirements` header (cumulative, NOT `## ADDED Requirements`)
- 5 ADDED requirements preserved verbatim from delta
- 12 scenarios preserved verbatim

`openspec spec validate apply-patch-tool` will check for the bare `## Requirements` header on the cumulative path; the delta path keeps `## ADDED Requirements` for delta semantics.

---

## 5. Open Items

None blocking. The change is merged into master at `1509339` and the `feat/apply-patch-tool` branch was deleted post-merge.

**Tracked follow-ups (out of scope, documented in `tasks.md` §后续跟踪)**:
- `path.rs` `safe_canonicalize` could be optimized (current O(n) component traversal)
- `apply_hunks` `find_hunk` only supports basic fallback; could add fuzzy matching (codex has 163-line `seek_sequence.rs` dedicated to this)
- Enabling `enable_move` in production requires D2 atomic rollback design + cross-filesystem `mv` compatibility tests + Guardian policy extension (D2.5 default-disabled preserved)
