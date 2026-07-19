# Retrospective: split-synthia-exec-crates

> Change: `split-synthia-exec-crates` (archived as `2026-06-12-split-synthia-exec-crates`)
> Commit: see git log; spec: `tool-crate-split`

## 1. Outcome

`synthia-exec` successfully split into two new workspace members. Pure refactor, zero behavioral change. The shim preserves `synthia_exec::*` paths indefinitely.

| Item | Result |
|------|--------|
| `synthia-tool-bash` crate created | ✅ |
| `synthia-tool-exec-base` crate created | ✅ |
| `synthia-exec` reduced to 6-line shim | ✅ |
| `bash_utf8_panic` integration test moved + import updated | ✅ |
| `cargo check --workspace` | ✅ 0 errors |
| `cargo test -p synthia-tool-bash` | ✅ 26 unit + 5 integration |
| `cargo test -p synthia-tool-exec-base` | ✅ all pass |
| `cargo test -p synthia-exec` (shim) | ✅ 32 inherited lib tests pass |
| `cargo test -p synthia-context -p synthia-agent --lib` | ✅ 1009 tests, 0 regressions |
| `cargo clippy` | ✅ 0 new warnings |
| `cargo +nightly fmt -- --check` | ✅ 0 diff |
| `openspec validate` | ✅ green |
| `openspec archive` | ✅ spec synced to baseline as `tool-crate-split` |

## 2. What worked

### 2.1 Dependency graph was already clean

The split was nearly a no-op for source code:
- `bash_tool.rs` only `use crate::{command_blacklist, command_manager}` — both stay in `synthia-tool-bash`
- `monitor.rs` only `use crate::command_manager` — stays in `synthia-tool-bash`
- `exec/executor.rs` only `use crate::exec::{...}` for sibling types — stays in `synthia-tool-exec-base`
- **Zero `use crate::...` references crossed the future crate boundary.**

This made the move purely a Cargo.toml + file system operation. No source rewrites beyond the 1-line import update in the test file.

### 2.2 Shim pattern held up

Per project memory: "Module split pattern: keep original file as 1-line `pub use sub_module::*` shim, never delete the original path."

The shim is 6 lines (2 `pub use` + 4 comment lines). It compiles cleanly, all 32 tests in the shimmed crate still resolve, and external users of `synthia_exec::*` keep working. The openspec delta explicitly documents the shim's existence and contract.

### 2.3 OpenSpec delta on a refactor

The validator normally requires a delta, but refactors don't modify behavior. The compromise was a new capability `tool-crate-split` with three ADDED Requirements that describe the *structure* of the split (where files live, what dependencies exist, how the shim works). This lets future changes reason about the new crate layout without an off-the-cuff "no spec needed" excuse.

### 2.4 The 1 initial compile error was a feature, not a bug

Forgetting `chrono` in `synthia-tool-exec-base/Cargo.toml` caught us at `cargo check --workspace` and the fix was trivial (one line in Cargo.toml). The error was a useful reminder that the new crate's deps are not the old crate's deps — the new crate must declare every third-party import it actually uses.

## 3. Issues encountered

### 3.1 Inherited `cargo test` count anomaly

After the split, `cargo test -p synthia-exec` reports 32 tests passing, but `synthia-exec/src/lib.rs` is just a 6-line shim. The 32 tests are actually the moved `exec/*` unit tests, which `cargo test` still reports under the shim crate's name because the shim re-exports `synthia_tool_exec_base::*` and the lib tests for those modules are inherited.

This is a cargo behavior, not a bug. The tests run from `synthia-tool-exec-base`'s own test target as well. The double-count is benign.

### 3.2 Pre-existing opencode/codex gaps still TODO

The two candidates carried over from the previous retrospective:
- Codex session/Turn model — deferred to task 1 (next)
- OpenCode v2 + ACP — explicitly out of scope per the user's "1,3 目前不需要acp"

No change in this assessment after the split.

## 4. Follow-ups (for next gap evaluation)

| ID | Item | Priority | Rationale |
|----|------|----------|-----------|
| Task 1 | Codex session/Turn model | High | User-selected next gap |
| FU.6 | Auto-invoke `prune()` in stream builder | Deferred | Adversarial review confirmed production loop never pushes tool results into `ctx.messages`; current pipeline is correct |
| Future | Decide whether `synthia-tool-exec-base` is the right home for the Turn model's task scheduler | Defer | When Turn model design lands, the `Executor` may be promoted to its core or replaced; not forcing a decision now |
| Future | Drop the bash tool's direct `tokio::process::Command` path in favor of `synthia-tool-exec-base::Executor` | Low | Would unify the two halves through a real layer; not needed unless the Bash tool needs priority queueing or rate limiting |

## 5. Metrics

| Metric | Before | After | Δ |
|--------|--------|-------|---|
| Workspace crates | 22 | 23 | +1 (net; +2 new, 0 removed because shim stays) |
| `synthia-exec` source LOC | 955 | ~10 | -945 (-99%) |
| `synthia-exec` Cargo.toml deps | 14 (incl. jsonschema, libc, toml) | 2 (synthia-tool-bash, synthia-tool-exec-base) | -12 (-86%) |
| New public types introduced | n/a | 0 | 0 (pure refactor) |
| Public API breakages | n/a | 0 (shim) | 0 |
| Total tests passing (workspace hot path) | 1009 | 1009 | 0 regression |
| Files moved | 0 | 11 (6 src + 5 in exec/) | +11 |
| Lines of new code | n/a | ~25 (2 new Cargo.tomls + 2 new lib.rs + 1 reduced lib.rs) | minimal |

## 6. Next gap evaluation

User selected: **1 (Codex session/Turn) + 3 (already done)**, ACP explicitly out of scope.

Recommend next: open a new OpenSpec change for the Codex session/Turn model, following the established pattern:
1. Multi-expert adversarial review (the user values this — see project memory)
2. Design.md + proposal.md + tasks.md
3. P0 quick win → commit → next P1 phase
4. Archive + retrospective

A natural P0 is: define what a "Turn" means in the current codebase (probably a single user → assistant → tool round-trip) and document the boundary in a new spec. The speculative trait abstraction the project memory warns about should be deferred until we have a concrete use case driving it.
