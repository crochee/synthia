# R40 Results — 2026-09-12

Predecessor: [`optimization-report-R39-2026-09-12.md`](optimization-report-R39-2026-09-12.md)
(`998f3084`).

R39 gave the loop a `Spawner` seam. R40 makes each crate say what it
needs from the runtime it runs on — and turns the leanness promises
into `make` targets, so the next round cannot quietly undo them.

## What the audit found

| Finding | Evidence |
|---|---|
| **Every crate inherited `tokio = { features = ["full"] }`** from `[workspace.dependencies]`, so `synthia-session` compiled tokio's `net`, `signal`, `process`, and `fs` subsystems to write a JSONL file, and `synthia-test-support`'s fakes compiled a runtime they never spawn on | `Cargo.toml`: `tokio = { version = "1.53", features = ["full"] }`, inherited by 13 crates |
| Test-only users declared tokio in dev-dependencies (good) but with no feature list, so they inherited the same `full` | `context`, `steering`, `skill`, `rag`, `eval`, `macros`, `workflow` |
| Nothing enforced the two promises the last three rounds established: the MVP subset pulls no HTTP client, and the pieces documented as "the runtime is yours" really are runtime-free | both were claims in docs, not gates |

## What landed

### A. Per-crate runtime features

`[workspace.dependencies]` no longer says `full`, and each crate's
`tokio` line names exactly the subsystems its own code uses, with a
one-line reason:

| Crate | Features | Why |
|---|---|---|
| `synthia-provider` | `macros`, `time` | `select!` in the SSE pump; retry backoff and the per-read idle timeout |
| `synthia-session` | `rt`, `sync`, `time`, `macros` | spawn_blocking, channels, deadlines |
| `synthia-test-support` | `sync` | its fakes never spawn |
| `synthia-tool`, `synthia-agent` | `rt`, `sync`, `time`, `fs`, `process`, `io-util`, `macros` | the builtin tools and the delegation gate genuinely run child processes and touch files |
| `synthia-server` | `rt-multi-thread`, `macros`, `net`, `signal`, `sync`, `time`, `io-util`, `fs` | the one application crate |
| `synthia-core`, `context`, `steering`, `skill`, `rag`, `eval`, `macros`, `workflow` | test-only, `macros`/`rt`/`rt-multi-thread` | their *tests* need an executor, their libraries do not |

### B. Two gates instead of two promises

- `make check-mvp-deps` (R38/R39) — the seven-feature subset pulls no
  `reqwest` / `hyper` / `rustls` / `h2` / `tower` / `opentelemetry` /
  `tonic` / `axum` / `rusqlite` / `sqlx`, and
  `synthia --no-default-features` compiles.
- `make check-no-runtime` (**new**) — `synthia-core`,
  `synthia-scheduler`, `synthia-macros`, `synthia-eval`,
  `synthia-workflow`, and `synthia-telemetry` (`--no-default-features`)
  pull no tokio in a **lib** build. Those are exactly the pieces whose
  docs say "the runtime lives in the caller" (the scheduler takes
  `tick(now)`, the workflow takes an injected host, the macro generates
  code); the target fails the day one of them inherits tokio again.

```
$ make check-no-runtime
OK: synthia-core synthia-scheduler synthia-macros synthia-eval synthia-workflow
    and synthia-telemetry --no-default-features are tokio-free
```

### C. Honest measurement of what narrowing bought

Diffing the MVP subset's lib tree before/after (a scratch worktree at
`998f3084` versus the working tree):

```
before=124 after=123
removed: socket2
```

So the crate-count effect is **one crate**, and the report says so
rather than quoting a bigger number. The real effect is the *feature*
surface tokio is compiled with: `net`, `signal`, `io-std`,
`parking_lot`, and `rt-multi-thread` are gone from lib builds (they
remain only where the application crate asks for them). `mio` and
`signal-hook-registry` stay because the builtin `shell` tool enables
tokio's `process` — which is the honest boundary, and the reason
R41's first item is a runtime seam *for the builtin tools*.

## Verification

| Check | Result |
|---|---|
| `cargo +nightly fmt --all` / `--check` | clean |
| `cargo clippy --all-targets --all-features --tests --all -- -D warnings` | 0 warnings, 0 errors |
| `cargo check -p <crate> --all-targets` for all 19 crates | 19/19 ok (this is what caught the seven crates whose examples/tests needed `rt-multi-thread`) |
| `cargo check --workspace --all-targets` | ok |
| Per-crate tests | **2585 passed / 0 failed** (identical totals to R39 — the change is dependency-side) |
| `make check-mvp-deps` | green (both checks) |
| `make check-no-runtime` | green |
| `cd docs/examples/minimal-consumer && cargo run` | `MVP-OK` |
| `cd docs/examples/external-consumer && cargo run` | `CONSUMER-PROOF: OK` |
| Every example, 30/30 | exit 0 |

Per-crate totals: core 107, telemetry 36, provider 744, context 95,
tool 307, session 137, steering 74, skill 56, agent 306, attachment 15,
mcp 52, rag 38, scheduler 14, macros 31, eval 36, workflow 65,
test-support 18, facade 17, server 437.

## Deferred to R41 (recorded, not dropped)

- **A runtime seam for the builtin tools.** `read` / `write` / `shell`
  use `tokio::fs` and `tokio::process` directly, which is what keeps
  `mio` + `signal-hook-registry` in every consumer's tree. An
  `AsyncFs` / `CommandRunner` pair (with a tokio-backed default) would
  let a consumer drop them the same way they now drop the HTTP client —
  and would make the `shell` tool's `CommandRunner` seam (R29) the same
  kind of injectable component as the rest.
- **A `Timer` seam** (carried from R39): provider retry backoff and the
  streaming idle watchdog still call `tokio::time`.
- **`synthia-server`'s `middleware/trace_context.rs`** still
  reimplements `synthia_telemetry::propagation` (carried from R37–R39).
- **OTLP lives behind `otlp`; consider whether `opentelemetry_sdk`'s
  `rt-tokio` is avoidable** when the crate is used with a non-tokio
  runtime — currently the exporter is the one part of the observability
  stack that needs a reactor.
