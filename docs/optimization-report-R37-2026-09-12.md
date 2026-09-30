# R37 Absorption Results — 2026-09-12

Plan: the audit below plus [`MINIMAL.md`](../MINIMAL.md).
Predecessor: R36 (`7998649c` Clock + IdGen abstractions, `36c1f936`
`ReActAgent` clock field).

R37 attacked the gap between this repo's stated purpose — *"cargo add
synthia and assemble an agent from zero"* — and what a consumer
actually had to **write** and **compile** to do it. The promise was
documented; it was never compiled.

## What the audit found

| Finding | Evidence |
|---|---|
| **`ModelProvider::embed` was a required method**, so every provider that has no embedding endpoint still had to write a body for it | twelve implementations across the workspace: seven test fixtures in `synthia-agent`, `ModelProviderStub`, `ReplayProvider`, the facade's assembly example, the server's test fake (all returning `Ok(vec![])` or dummy vectors) and `AnthropicProvider` (a hardcoded `Err("does not support embedding")`) |
| **The "start with almost nothing" claim was never compiled.** No crate in the tree built the minimal feature subset; the only "library consumer" proof (`external-consumer`) depends on nine crates directly | `docs/examples/external-consumer/Cargo.toml`; no example/test anywhere used `synthia` with `default-features = false` |
| **`synthia-telemetry` was all-or-nothing**: no cargo features at all, so `synthia-tool` — and therefore *every* agent consumer — compiled the OTel SDK, `tracing-opentelemetry`, and both OTLP transports (tonic gRPC + reqwest HTTP) to record three Prometheus counters | `crates/synthia-telemetry/Cargo.toml` had no `[features]`; the root manifest's own comment claimed the opposite: *"Crate-level feature gates (`otel`, `metrics`) live in `crates/synthia-telemetry/Cargo.toml`"* |
| **`synthia-telemetry` declared two dependencies it never used** | `uuid` and `tokio` appear in `Cargo.toml` and nowhere in `src/` or `tests/` |
| **`synthia-core` — the crate whose pitch is "no runtime, no provider SDK" — hard-depended on `reqwest` and `tokio`** | `reqwest` for exactly one impl (`From<reqwest::Error> for Error`, `error.rs`); `tokio` only in `#[tokio::test]` |
| **Setting `SYNTHIA_OTLP_ENDPOINT` silenced every console log** | the OTLP registry installed `filter + telemetry_layer` and no `fmt` layer, so `tracing::info!` lines went to the collector and nowhere else — a boot with a misconfigured endpoint produced a silent process |

## What landed

### A. `embed` is a defaulted trait method

`ModelProvider::embed` now returns one empty vector per input text by
default, with the contract documented and pinned by a test
(`embed_returns_empty_vectors_per_input`, `embed_returns_empty_vectors`).
Providers that do support dense embeddings override it;
`ModelProviderEmbedder` in `synthia-rag` is the only caller, and it goes
through its own `Embedder` trait, so nothing else changes. The two fakes
whose tests had *pinned* the old behaviour (a stub returning `vec![]`, a
replay provider returning an error) were re-pointed at the new contract
deliberately — the tests now document that falling through is intended,
not accidental. `AnthropicProvider`'s hardcoded error impl is gone.

One override survives on purpose: `synthia-test-support`'s
`FakeProvider` keeps returning 1536-dimensional dummy vectors, because
it is the *usable* public fake — a consumer driving
`ModelProviderEmbedder` in their own tests wants vectors with a shape,
and no test in this workspace depends on it.

### B. The minimal agent is a program, not a paragraph

`docs/examples/minimal-consumer/` — an independent crate (empty
`[workspace]` table, like `external-consumer`) with **one** dependency:

```toml
synthia = { path = "…/crates/synthia", default-features = false, features = [
  "core", "provider", "context", "tool", "session", "steering", "agent",
] }
```

It implements a scripted provider that *asks for a tool* on the first
call and answers on the second, registers the builtin registry plus one
hand-written `Tool`, and asserts what a consumer observes: the tool ran
exactly once, the model was called twice, the answer reached the caller,
and structural events arrived on the typed sink.

`MINIMAL.md` is the matching 4-step guide (decide → provider → assemble
→ add pieces), written so a consumer who wants an MVP can stop reading
after step 2.

### C. The leanness promise is now a gate

`make check-mvp-deps` resolves the same seven-feature subset and fails
if `opentelemetry`, `opentelemetry-otlp`, `tonic`, `axum`, `rusqlite`,
or `sqlx` re-enters the tree. Verified in both directions: green on the
seven-feature subset, red when `telemetry` is added back.

### D. Observability became two switchable halves

`synthia-telemetry` now has real features instead of a promise of them:

| Piece | Feature | Contents |
|---|---|---|
| console + file logging | always compiled | `tracing`, `tracing-subscriber` |
| Prometheus vectors | `metrics` | `prometheus`, `lazy_static` |
| OTLP export + W3C propagation | `otlp` | `opentelemetry*`, `tracing-opentelemetry`, tonic + reqwest transports |

Code moved with it: console/file tracing left `tracer.rs` for
`console.rs`; `tracer.rs` is now OTLP-only; `propagation` and `tracer`
are gated together (W3C extraction only means something with an SDK to
hand the span context to); the init result was renamed
`TracerInitResult` → `TracingInit`, whose `Otlp` variant exists only
with the feature.

In-tree members state what they use: `synthia-tool` takes
`default-features = false, features = ["metrics"]`, `synthia-server`
names `["metrics", "otlp"]` explicitly (the server's observability
stays on), and the facade's `telemetry` feature turns on both.

### E. Dead dependencies removed

`uuid` + `tokio` deleted from `synthia-telemetry`; `tokio` demoted to a
dev-dependency and `reqwest` made an opt-in feature of `synthia-core`,
enabled only by `synthia-provider` (the one crate that uses
`Error::from(reqwest::Error)`).

### F. The OTLP pipeline keeps its logs

The OTLP registry now composes the console `fmt` layer alongside the
filter and the OTel layer, so enabling an exporter no longer costs an
operator every log line of a boot. This was found by pointing the real
server at an OTLP endpoint during verification (below).

## Verification

| Check | Result |
|---|---|
| `cargo +nightly fmt --all` | clean |
| `cargo clippy --all-targets --all-features --tests --all -- -D warnings` | 0 warnings, 0 errors |
| `cargo clippy -p synthia-telemetry --no-default-features --all-targets -- -D warnings` | clean (console-only build) |
| `cargo clippy -p synthia-telemetry --no-default-features --features metrics …` | clean (metrics-only build) |
| `cargo clippy -p synthia --no-default-features --features core,provider,context,tool,session,steering,agent …` | clean (MVP subset build) |
| **MVP feature subset dependency count** | **182 → 158 unique crates** (measured with `cargo tree` at `HEAD` in a scratch worktree vs the working tree) |
| `cd docs/examples/minimal-consumer && cargo run` | `model calls: 2`, `tool calls: 1`, `structural events: 12`, `MVP-OK` |
| `cd docs/examples/external-consumer && cargo run` | `CONSUMER-PROOF: OK` |
| `make check-mvp-deps` | `OK: core,provider,context,tool,session,steering,agent pulls no (opentelemetry|opentelemetry-otlp|tonic|axum|rusqlite|sqlx)` |
| `make check-mvp-deps MVP_FEATURES=…,telemetry` (the gate must be able to fail) | `FAIL: … opentelemetry opentelemetry-otlp tonic` + exit 1 |
| Per-crate tests | see the sweep table below |
| Real server boot, console path | `/livez` 200, `/readyz` 200, `/metrics` 200 with `http_requests_duration_seconds_bucket{method="GET",path="/api/v1/tools"}` |
| Real server boot, `SYNTHIA_OTLP_ENDPOINT=http://127.0.0.1:4318 SYNTHIA_OTEL_SAMPLER=trace_id_ratio:0.1` | logs `OpenTelemetry OTLP tracing initialized endpoint="http://127.0.0.1:4318" service="synthia-server" protocol=Http sampler="trace_id_ratio:0.1"`, then serves `/livez` 200 and `/metrics` 200 |

Per-crate test totals after the round (credential-free sweep,
`cargo test -p <crate>` with default features — the command the
credential-free sweep uses):

| Crate | Passed | Crate | Passed |
|---|---|---|---|
| core | 104 | attachment | 15 |
| telemetry | 36 | mcp | 52 |
| provider | 743 | rag | 38 |
| context | 95 | scheduler | 14 |
| tool | 307 | macros | 31 |
| session | 137 | eval | 36 |
| steering | 74 | workflow | 65 |
| skill | 56 | test-support | 18 |
| agent | 306 | facade | 14 |
| | | server | 437 |

**2578 passed / 0 failed.** The affected crates were additionally run
with `--all-features` (core 106, provider 747, tool 307, telemetry 36)
and the telemetry feature subsets were run separately
(`--no-default-features` 1, `--features metrics` 4) — all green.

## Deferred to R38 (recorded, not dropped)

- **`synthia-provider`'s HTTP adapters are still unconditional.** The
  MVP tree lost the OTel/tonic/axum weight but still compiles
  `reqwest` + `hyper` + `rustls` for `AnthropicProvider` /
  `OpenAICompatibleProvider`, which a consumer who implements their own
  `ModelProvider` never touches. Gating them behind a `http` feature
  (with `synthia-server` opting in) is the next big leanness win.
- **`tokio = { features = ["full"] }` at the workspace level** means
  every crate that inherits it compiles `process`, `signal`, `net`,
  `fs`, and `io-std` whether or not it uses them. Narrowing the shared
  entry to a default set plus per-crate additions is mechanical but
  touches every manifest.
- **`synthia-server`'s `middleware/trace_context.rs` reimplements
  `synthia_telemetry::propagation`** (its own
  `register_global_propagator` / `extract_trace_context` /
  `inject_trace_context` over the same `opentelemetry` globals). Now
  that the telemetry crate is feature-gated rather than all-or-nothing,
  the server can call the shared implementation and drop its copy.
- **`synthia-agent` / `synthia-session` / `synthia-tool` still link
  `tokio` for spawn, timers, fs, and process** — R23 removed the last
  `tokio` *type* from a public signature, but a runtime-neutral
  composition still means "an executor exists somewhere". Making that a
  seam (a `Spawn`/`Timer` trait pair in `synthia-core`) is a design
  round of its own, not a dependency edit.
