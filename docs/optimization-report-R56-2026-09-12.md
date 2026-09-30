# R56 Results — 2026-09-12

Predecessor: [`optimization-report-R55-2026-09-12.md`](optimization-report-R55-2026-09-12.md)
(`dc9fe2fd`).

This repo's premise is that its documentation *is* the assembly tutorial
(`MINIMAL.md`, `SEAMS.md`, 31 examples, two standalone consumer crates).
Two holes in that premise, found by auditing the docs the way the
previous rounds audited the config.

## What the audit found

| Finding | Evidence |
|---|---|
| **No index over `docs/`.** 47 markdown files — 30 optimization reports, four subdirectories, the gap analysis, the contract — and nothing that says where to start or which report covers what | `ls docs/` |
| **Three crates had zero crate-level documentation**: `synthia-tool` (one of the seven core pieces), `synthia-server` (the application crate), `synthia-test-support` (the crate a consumer adds to test *their* assembly) | `src/lib.rs` with no leading `//!` lines; the other 16 crates have 12–232 of them |
| **The deferred list lived only in report tails.** Each round recorded what it did not do in the last section of its own report; reading "what is still open" meant reading 19 files in order | the reports R37–R55 |
| Docs that rot are invisible: R47 fixed two non-compiling `MINIMAL.md` recipes *by hand*, because nothing checks them | `optimization-report-R47-2026-09-12.md` |

`synthia-tool`'s silence is the striking one: it owns the tool registry,
the surface projection, the builtins, the sandbox policy and the mutation
queue — the questions a deployment asks most often ("what can the model
call, and what does a call touch?").

## What landed

### A. `docs/README.md` — the map

- **Reading order** for a new consumer: `MINIMAL.md` (smallest complete
  agent) → `SEAMS.md` (the seven pieces and their traits) →
  `docs/examples/README.md` → `README.md` → `DEPLOYMENT.md`, with the
  per-crate contracts named.
- **Layout**: what each directory is (`architecture/adr`,
  `interface-contract`, `design`, `examples`, `superpowers`, the
  reports).
- **Every round, grouped by theme** rather than by number — foundations
  (R3–R13), absorption (R14–R29), the pieces it named (R30–R34), the
  clock/id seams (R35–R36), library + runtime neutrality (R37–R41),
  performance (R42–R44/R48/R51–R52), the reasoning-loop seam
  (R49–R50), and the deployment surface (R53–R55) — each with a pointer
  to the report to read.
- **Known gaps (as of R55)**: one table, ten rows, each with *why* it is
  still open (not just what), plus the verification that R30–R34's
  deferrals all landed in R31–R34 rather than being quietly dropped.
- Linked from the root README next to the examples index.

### B. Crate docs where there were none

| Crate | What its docs now say |
|---|---|
| `synthia-tool` | The crate's load-bearing decision up front — **registered** / **shown** / **runnable** are three questions with three mechanisms — then a compiling example that implements a `Tool`, registers it, and asserts it reaches the model's advertised schema; plus what is deliberately absent (no runtime choice, no permission prompts, no retries) |
| `synthia-server` | The boot diagram (args + env → one config resolution → `AppState`), the route surfaces (chat SSE, management, probes), the module map, and the reason the crate is *not* part of the library assembly story |
| `synthia-test-support` | The four fakes and when to reach for each, with a compiling `FakeProvider` example — a consumer's dev-dependency for running their own agent offline |

Each example compiles as a doctest (`cargo test --doc` runs it), so the
docs are executable rather than illustrative — the same standard the
facade's 11 `compile_fail` doctests hold.

## Verification

| Check | Result |
|---|---|
| `cargo test -p synthia-tool --doc` | 3 passed (the new example compiles *and* passes) |
| `cargo test -p synthia-test-support --doc` | 2 passed |
| `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace` | **0 warnings** (was 74) — and the new `make doc-check` gate runs it |
| `crates/synthia-tool`, `synthia-server`, `synthia-test-support` crate docs | new, with compiling examples |
| `cargo +nightly fmt --all` / `make fmt-check` | clean |
| `cargo clippy --all-targets --all-features --tests --all -- -D warnings` | 0 warnings, 0 errors (so the new docs pass `clippy::doc_markdown` and friends) |
| Per-crate sweep (19 crates) | **2604 passed / 0 failed**; `synthia-context --features sqlite` 103 |
| `make ci` | green |
| Every link in `docs/README.md` | points at a file that exists (checked while writing; the table names reports by file) |

## Bonus: the docs were also *wrong*, not just missing

Adding the index exposed a second problem, measured by a gate rather
than by reading: `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
--workspace` reported **74 rustdoc warnings** — broken intra-doc links,
links to private items, redundant link targets, and raw `<T>` in prose.

They are not cosmetic. A sample of what the docs claimed and the code
did not:

| Doc said | Reality |
|---|---|
| `synthia_core`'s error module: "the stable wire code is `Error::kind`" | no such method exists; the wire code is assigned at the server boundary (the same file says so 370 lines later) |
| `hook_map.rs`: "the agent loop is wired to consult the hook map" / "attach it via `Steering::with_hook_map`" | the loop reads `Steering::hooks` and never consults a `HookMap`; `Steering` has no `with_hook_map` |
| `router.rs`: "Default impls use `SequentialRouter`" (printed twice) | no such type; the no-router behaviour is `PassThroughRouter` |
| `re_act.rs`: `synthia_provider::NoopContextManager` | `NoopContextManager` lives in `synthia_context` |
| `tool/registry.rs`: `ToolRegistry::register` / `register_scoped` / `register_scoped_with_namespace` | `register_entry` / `register_scoped_arc` / `create_session_scope` |

All 74 are fixed (five parallel agents, one per disjoint file set, each
with the same rule: fix the link, never suppress it; check whether the
item exists before unlinking). The four rows above were then corrected
by hand, because the right fix for a *stale sentence* is not an unlink.

### The gate

- **`make doc-check`** — `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
  --workspace`. Added to `make ci` (so it runs locally and in CI's
  `gates` job), and to the README's command list.

## Still open after this round

The known-gaps table in `docs/README.md` is now the single place to read
them; the notable ones are unchanged and recorded there — per-agent tool
restriction (a feature, not a fix), the builtin tools' runtime seam
(declined by design, R40), the `Timer` seam (R39), the projection's
`Arc<Value>` schema (R51), loop-level benchmark ratios (R52),
`cargo-deny` in CI (R41), a doctest gate over `SEAMS.md` (R47), an
in-tree LLM-judge scorer and a per-run strategy override over HTTP
(R50).
