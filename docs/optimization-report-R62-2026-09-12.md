# R62 Results — 2026-09-12

Predecessor: [`optimization-report-R61-2026-09-12.md`](optimization-report-R61-2026-09-12.md)
(`6844bd27`).

The objective has two axes this session had not audited end-to-end: **no
runtime in a library's public API** (AGENTS.md §3.7 states the rule; only
dependency-level gates existed) and **the reference projects' capabilities
are absorbed or consciously declined** (`Cargo.toml`/`SEAMS.md` claim
parity; nothing checked it).

## What the audit found

| Finding | Evidence |
|---|---|
| **A public struct in a library crate held a tokio handle.** `synthia_tool::truncate::CleanupTask` was `(Option<tokio::task::JoinHandle<()>>)` — in the very type whose doc comment claims "Runtime-neutral by construction: no `tokio` type appears in this crate's public API" | `crates/synthia-tool/src/truncate/bound_output.rs:178` |
| **Nothing enforced the rule.** The runtime gates were `check-no-runtime` (five crates are tokio-free *as dependencies*) and `check-mvp-deps` (the MVP tree pulls no HTTP/DB stack). A runtime type in a *signature* was invisible to both | `Makefile` |
| **The reference parity claim was unverified.** `docs/traitclaw-gap-analysis.md` (Chinese, an absorption *history*) is the only record; no current-state map said which traitclaw crate or pi package maps to what, and which are declined | 14 traitclaw crates, 11 pi packages |
| **One capability is genuinely missing**: pi's `agent/search` — a session/entry full-text search service (`SearchQuery`, `SessionSearchHit`, `EntrySearchHit`, `SessionSearchService::{searchSessions, searchEntries, sync, notify, remove, close}`) | `~/workspace/pi/packages/agent/src/search/index.ts` |

## What landed

### A. `CleanupTask` is runtime-neutral (and its doc is true again)

```rust
pub struct CleanupTask(Option<Box<dyn Fn() + Send + Sync>>);   // was JoinHandle

impl CleanupTask {
    pub fn from_stop(stop: impl Fn() + Send + Sync + 'static) -> Self;
    pub fn abort(&self);     // idempotent
    pub fn detach(self);
}
```

- The stop path is a **callback**; `start_cleanup_task` (the tokio-bound
  constructor, unchanged in behaviour) wraps `handle.abort()` in it, so
  the runtime type stops at the crate boundary.
- `from_stop` is a real seam, not a test hook: a consumer running their own
  cleanup loop on another executor keeps the same drop-stops-it contract.
- `abort` is idempotent **by construction** (a once-flag inside the
  closure). The old `JoinHandle::abort` had that property incidentally;
  a callback needs it on purpose — the new test caught exactly that (my
  first implementation called a user callback twice on `abort(); drop(…)`).
- `Debug` is manual (`armed: bool`) because `Box<dyn Fn()>` has none.

### B. `make check-public-api-runtime`

Strips line comments, then fails if any `pub` line in a library crate's
`src/` names `tokio::`, `async_std::`, `smol::` or `futures::executor::`.
`synthia-server` is exempt (the application crate, where a runtime is the
point). Part of `make ci` and of the `gates` CI job.

**It has teeth**: pointed at R61's source it reports
`pub struct CleanupTask(Option<tokio::task::JoinHandle<()>>);` — the gate
would have caught the leak it was written for.

### C. `docs/reference-parity.md` — the objective's first axis, checkable

Every traitclaw crate (14) and pi package (11) mapped to its synthia
counterpart, verified by resolving each counterpart symbol against the
library sources; the one miss is the one open gap. Plus the declines with
reasons (TUI, bundled coding agent, full MCTS, CBOR protocol,
chord/facets, builtin runtime seams) and the gates that keep the checkable
claims true.

Selection from the table:

| Reference | Verdict |
|---|---|
| traitclaw: core, steering, strategies (react/cot), team, mcp, rag (chunk/embed/hybrid/ground), memory-sqlite, eval, test-utils, macros, three adapters, facade, miniclaw | absorbed (verified) |
| traitclaw MCTS strategy | declined: the useful half is `BestOfNStrategy` / `Step::BestOf`; a full tree search is not worth its token cost |
| pi: harness, lcm/DAG, compaction, ai, server/client, evals, telemetry, session-backends | absorbed (verified) |
| pi `tui`, `coding-agent`, `protocol` (cbor), `chord` | declined with reasons |
| pi `agent/search` | **open gap → R63** |

## Verification

| Check | Result |
|---|---|
| `make check-public-api-runtime` | `OK` on the fixed tree; reports the `JoinHandle` line against R61's source |
| `make ci` | green (fmt, clippy, rustdoc `-D warnings`, MVP-deps, runtime-free, **new runtime-API gate**, clock ratchet) |
| `cargo test -p synthia-tool` | **313 passed / 0 failed** (312 before: +1 test for `from_stop`'s drop/abort/detach semantics) |
| `cargo test -p synthia-agent` / `synthia-server` / `synthia` | 330 / 434 / 20 passed, 0 failed |
| `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace` | 0 warnings |
| `cargo +nightly fmt --all`, clippy `-D warnings` | clean |

## Next: the one open gap

R63 — session search (pi parity) — closed (commits `7f63ba2`, `2a6185e`,
`c7e6e7d`, `c0aff40`, `26f369a`; see the R70 round-map row in
`docs/README.md` and the "What happened after R64" appendix in
`docs/optimization-report-R64-2026-09-14.md`). Runtime-neutral
snippets + scoring, `GET /api/v1/sessions/search` route,
`cargo run -p synthia-session --example session_search` →
`SESSION-SEARCH: OK`; all 4 integration tests pass.

Everything else stays as recorded in `docs/reference-parity.md` and
`docs/README.md` (known gaps): the two `synthia-web` lockfiles (maintainer
workflow decision), an in-tree LLM-judge scorer and a per-run strategy
override over HTTP (features), `cargo-deny` in CI (needs the binary and an
advisory fetch), `GroupedRegistry`'s residual name clones, and the
projection's remaining per-request schema clone (re-scoped by R59).
