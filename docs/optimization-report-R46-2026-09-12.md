# R46 Results — 2026-09-12

Predecessor: [`optimization-report-R45-2026-09-12.md`](optimization-report-R45-2026-09-12.md)
(`7f9f647e`).

R37–R45 changed what the framework compiles, what it requires from a
runtime, and what it measures. R46 is the consistency pass: four
documents still described the state before them, and one of the fixes
touches the crate a consumer adds first.

## What the audit found

| Finding | Evidence |
|---|---|
| **`crates/synthia-core/README.md` was three rounds stale.** Its feature list stopped at `error` / `registry` / `sensitive` / `text` / `token` — missing `cancel`, `spawn`, `clock`, `idgen`, `schema`, and `full_output`, i.e. most of what the foundation crate now owns (R36's Clock/IdGen, R39's Spawner, R7's cancel) | the file listed 6 modules; `lib.rs` exports 11 |
| **`crates/synthia-agent/README.md` documented the wrong cancellation type.** Its feature list and example were written around `tokio_util::sync::CancellationToken`, which R7 replaced with the `CancelToken` trait + std-only `AtomicCancelToken` | `use tokio_util::sync::CancellationToken;` in the README's only example |
| **`DEPLOYMENT.md` recommended the forbidden command.** Its test section said `make test-rust # cargo test --workspace`; R41 made that target per-crate and AGENTS.md §3.3 forbids the workspace-wide run | `DEPLOYMENT.md` lines 63–69 |
| The agent README never mentioned R39's runtime seam, so a reader could not learn from it that the executor is theirs | no `Spawner` / `runtime_agnostic` mention anywhere in the file |

## What landed

### A. `synthia-core`'s README is the foundation crate's map again

Rewritten around the eleven modules `lib.rs` actually exports, with the
crate's one rule stated up front (no runtime, no provider SDK, no
upward dependency), the full module table, and the "what is deliberately
not here" list. The `SharedClock` / `SharedIdGen` discipline — including
why every "stamps now" constructor has an explicit-`now` sibling — is
now documented where a consumer meets the crate, matching the
`make check-clock` gate that enforces it.

### B. The agent README describes the framework's own seams

- Cancellation: the `CancelToken` trait, `AtomicCancelToken` as the
  runtime-neutral default, and the tokio bridge as a coercion rather
  than the type to reach for.
- A new **Runtime contract** section: the `Spawner` seam for the
  per-turn detach, the `runtime_agnostic` example as the proof, and the
  same three-column table (neutral / tokio-bound plugins / needs a
  reactor today) the crate docs carry — one story, no drift between
  `lib.rs` and the README.
- `AgentBuilder` is named as the fluent alternative to the
  `ReActAgent::with_*` chain, with the setter list.

### C. `DEPLOYMENT.md` points at the gates that exist

The stale test block is replaced with `make ci` / `make test-crates` /
`make test-sqlite` / `make examples`, plus a line naming
`.github/workflows/rust-quality.yml` and its three jobs.

## Verification

| Check | Result |
|---|---|
| `make ci` | green (fmt, clippy, MVP deps, runtime-free, clock ratchet) |
| `make examples` | every example plus both consumer crates: `MVP-OK`, `CONSUMER-PROOF: OK` |
| Stale-claim grep (`synthia-server/otel`, `TracerInitResult`, `13 libraries`, `5 builtins`, `no cargo features`, `cargo test --workspace` in current docs) | no hits outside historical `optimization-report-*` files, which are records of past rounds and are left as written |
| `cargo test --doc -p synthia` (default + MVP subset) | unchanged; no README is `include_str!`-ed into a crate, so README examples are illustrative by design |

## Deferred to R47 (recorded, not dropped)

- **A count accumulator to drop the per-message `String`** in
  `estimate_messages_token_count` (carried from R42/R43).
- **A runtime seam for the builtin tools** (carried from R40).
- **A `Timer` seam** (carried from R39).
- **`cargo-deny` in CI** (carried from R41) — `deny.toml` is empty and
  the binary is unavailable locally.
- **A seam index**: one page listing every trait a consumer can
  implement (`ModelProvider`, `Tool`, `ContextManager`, `Sink`,
  `CancelToken`, `Spawner`, `Retriever`, `Embedder`, `WorkflowHost`, …)
  and what each replaces. `docs/examples/README.md` plus the crate docs
  cover it piecemeal; a single index is the tutorial-shaped version of
  the same information.
