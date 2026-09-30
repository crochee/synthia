# R41 Results — 2026-09-12

Predecessor: [`optimization-report-R40-2026-09-12.md`](optimization-report-R40-2026-09-12.md)
(`b7d376c0`).

R37–R40 built five invariants and documented each one. Nothing ran
them automatically: `.github/workflows/` held a contract-closure
scanner and a Playwright job, and every Rust gate existed only as a
convention an agent was asked to follow.

## What the audit found

| Finding | Evidence |
|---|---|
| **No CI job ran fmt, clippy, or the test suite.** The repo's Rust quality rested entirely on whoever remembered to run it | `.github/workflows/` = `contract-closure.yml`, `e2e.yml` |
| The four invariants of R37–R40 had no execution path (`make check-mvp-deps`, `make check-no-runtime` — written, run by hand, never by a machine) | no workflow references them |
| `make test-rust` / `make test-unit` used `cargo test --workspace` — the one command AGENTS.md §3.3 forbids, shipped as the "CI-friendly" entry point | `Makefile` before this round |
| The README's Test section advertised that same workspace-wide command, and its project-structure tree still listed "13 libraries" and omitted `synthia`, `synthia-workflow`, and `synthia-eval` | `README.md` |

## What landed

### A. `.github/workflows/rust-quality.yml`

Three jobs, no API keys, every step a `make` target:

| Job | Steps |
|---|---|
| `gates` | `make ci` = `fmt-check` + `lint-rust` (`-D warnings`) + `check-mvp-deps` + `check-no-runtime` |
| `tests` | `make test-crates` (one member at a time), `make test-sqlite`, and the facade's feature-gating doc tests in **both** configurations (default and the seven-feature MVP subset, where the `compile_fail` proofs are live) |
| `examples` | `make examples` = every example plus both standalone consumer crates |

The workflow calls `make` rather than spelling commands out, so a red
CI step is reproducible with the same line locally, and the gate list
lives in one file. It follows the existing workflows' conventions
(`dtolnay/rust-toolchain`, cargo cache, `concurrency` group).

### B. Makefile: CI entry points, and no workspace-wide test run

- New: `fmt-check`, `ci`, `test-crates`, `test-sqlite`, `examples`.
- `CRATES := $(notdir $(wildcard crates/*))` — a new workspace member
  needs no Makefile edit.
- **`test-rust` and `test-unit` now run per crate.** `cargo test
  --workspace` no longer appears in the Makefile at all; the target that
  used to recommend it is an alias for the batched loop.
- `make examples` walks every `crates/*/examples/*.rs`, adds
  `--features sqlite` for the one example that needs it, and finishes
  with both standalone consumer crates.

### C. Docs caught up with the tree

- `AGENTS.md` §3.6 lists every gate target and states that CI calls
  them; §3.7's runtime-neutrality rule now names `Spawner` as the only
  detach seam and points at the two executable proofs
  (`runtime_agnostic` example, `check-no-runtime`).
- `README.md`'s Test section documents `make ci` / `test-crates` /
  `test-sqlite` / `examples` and which workflow runs them; the
  project-structure tree now lists all 19 members with a one-line
  responsibility each instead of a stale 13-crate list.

## Verification

| Check | Result |
|---|---|
| `make ci` | green: `fmt --check` clean, clippy `-D warnings` 0, both invariant targets OK |
| `make test-crates` | every member green, one crate at a time |
| `make test-sqlite` | green |
| `make test-unit` | every member's lib tests green |
| `make examples` | all 30 examples exit 0; `CONSUMER-PROOF: OK`; `MVP-OK` |
| Workflow YAML | parses; 3 jobs, 6/8/5 steps |
| `cargo clippy --all-targets --all-features --tests --all -- -D warnings` | 0 warnings, 0 errors |
| Per-crate totals | 2585 passed / 0 failed (unchanged from R40 — this round adds no behaviour) |

## Deferred to R42 (recorded, not dropped)

- **A runtime seam for the builtin tools** (carried from R40): `read` /
  `write` / `shell` still use `tokio::fs` / `tokio::process`, which is
  what keeps `mio` + `signal-hook-registry` in a consumer's tree.
- **A `Timer` seam** (carried from R39): provider retry backoff and the
  streaming idle watchdog still call `tokio::time`.
- **`synthia-server`'s `middleware/trace_context.rs`** still
  reimplements `synthia_telemetry::propagation` (carried from R37).
- **`cargo-deny` is not wired into CI**: `deny.toml` exists as a
  placeholder, and a licence/advisory job would need the config filled
  in first.
- **Benchmarks**: there is still no measurement harness, so "high
  performance" claims in the docs rest on design (no per-call
  allocation in the hot paths, `Arc<Vec<T>>` prompt-cache identity,
  chunked full-output retention) rather than on numbers.
