# R44 Results — 2026-09-12

Predecessor: [`optimization-report-R43-2026-09-12.md`](optimization-report-R43-2026-09-12.md)
(`f9610f9d`).

R44 closes two loops: the benchmark stops measuring the scheduler
instead of the code, and the server stops carrying a second copy of the
telemetry crate's W3C propagation.

## What the audit found

| Finding | Evidence |
|---|---|
| **The turn benchmark paid for an OS thread per run.** The pooled-executor case is what a deployment actually runs, and the spawn dominated the small deltas the harness was built to detect | bench spawner was `std::thread::spawn` + `block_on` per turn |
| **A single measurement pass is noise on a shared machine.** Two consecutive full runs differed by ~30% on *every* entry (including ones nothing had touched), so the harness could not confirm or refute a small change | e.g. `estimate_token_count` 64.7 ns then 94.9 ns with no code change between them |
| **`synthia-server`'s middleware carried its own copy of the telemetry crate's W3C propagation** — the propagator install, the idempotence flag, the extract/inject shims, and its own `ExtractedTraceContext` / `InjectedTraceContext` structs, all duplicating `synthia_telemetry::propagation` | `crates/synthia-server/src/middleware/trace_context.rs`; the telemetry module's own docs claimed the server "does not parse or format `traceparent` itself", which had stopped being true |

## What landed

### A. The harness measures the code, not the scheduler

- The turn bench runs on a `futures::executor::ThreadPool` (not tokio,
  and no thread spawn per run).
- Every entry runs five passes and reports the **minimum** per-op time —
  the standard estimator for "how fast is this when nothing
  interferes". The header says so, and says to compare only against the
  same machine before the change.
- Re-measured with the stable harness: `descriptors_cached` **9.6 ns**
  against `descriptors` **17 519 ns** (1825×) on a 40-tool registry;
  the scripted turn **262 µs**; the eviction path **484 µs**.

### B. The server uses the telemetry crate's propagation

`middleware/trace_context.rs` shrank to the HTTP layer it should own:

| Removed | Kept |
|---|---|
| `register_global_propagator`, `ensure_global_propagator_installed` | the axum `HeaderMapExtractor` / `HeaderMapInjector` adapters |
| `extract_trace_context`, `inject_trace_context` | the middleware itself (span stamping, `tracestate`-without-`traceparent` passthrough) |
| local `ExtractedTraceContext` / `InjectedTraceContext` | `new_trace_id` / `new_span_id` (minting this hop's ids is server business) |

The shared version is a superset (its extracted context carries the
parent span id too), the middleware's behaviour is unchanged, and the
telemetry module doc that promised this arrangement is true again. The
existing middleware tests — which drive a real `axum::Router` and
assert `traceparent` round-trips, `tracestate` passthrough and the
generated-header case — pass untouched, which is the evidence that the
deletion was behaviour-preserving.

### C. Measured and declined: caching the projection

`project_tool_definitions` is now the largest per-iteration cost at
**26.6 µs** for 40 tools (~10% of the 262 µs turn). Caching it needs a
key of (registry version, visible filter, deferred promotion state),
a new stateful cache type reachable from the loop, and an invalidation
story for the transcript-derived part — for a saving that is 0.01% of a
real turn, where the model call is hundreds of milliseconds. The
measurement is recorded here and in the code comment on
`compose_tool_definitions` rather than paid for in complexity.

## Verification

| Check | Result |
|---|---|
| `cargo +nightly fmt --all` / `--check` | clean |
| `cargo clippy --all-targets --all-features --tests --all -- -D warnings` | 0 warnings, 0 errors |
| `cargo test -p synthia-server` | **437 passed / 0 failed**; the 4 `middleware::trace_context` tests pass untouched |
| `cargo test -p synthia-agent` | 306 passed / 0 failed |
| `cargo bench -p synthia-agent --bench hot_paths` | stable table (min-of-5), above |
| `make ci` / `make examples` | green |

## Deferred to R45 (recorded, not dropped)

- **Direct `chrono::Utc::now()` in production paths** — an audit found
  calls that bypass `synthia_core::Clock` in `synthia-attachment`
  (`saved_at`), `synthia-context`'s memory tiers (`created_at`),
  `synthia-eval`'s report (`generated_at`), `synthia-session`'s
  `OperationSnapshot::taken_at`, and four sites in the server's session
  controller. AGENTS.md §3.8 forbids exactly this in new code; the fix
  is to thread `SharedClock` into those components (which also makes
  their timestamps testable). `synthia-provider`'s `parse_retry_after`
  reads the wall clock to turn an HTTP date into a delay — plumbing a
  clock there changes a public signature, so it needs its own decision.
- **A count accumulator to drop the per-message `String`** in
  `estimate_messages_token_count` (carried from R42/R43).
- **A runtime seam for the builtin tools** (carried from R40).
- **A `Timer` seam** (carried from R39).
- **`cargo-deny` in CI** (carried from R41) — `deny.toml` is still
  empty and the binary is not installed locally, so wiring it would be
  an unverifiable gate.
