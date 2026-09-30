# R38 Absorption Results — 2026-09-12

Predecessor: [`optimization-report-R37-2026-09-12.md`](optimization-report-R37-2026-09-12.md)
(`03a790d3`).

R37 proved the smallest agent compiles and runs, and cut the OTel stack
out of it. It was still linking the whole HTTP stack — `reqwest`,
`hyper`, `h2`, `rustls`, `tower` — because three dependency edges were
unconditional: `synthia-tool → reqwest` (for `web_fetch`),
`synthia-provider → reqwest` (for the two adapters), and
`synthia-provider → synthia-core/reqwest` (for one `From` impl). A
consumer who implements their own `ModelProvider` and never fetches a
URL compiled all of it.

## What the audit found

| Finding | Evidence |
|---|---|
| The bundled HTTP adapters were unconditional, so `provider` could not mean "the model seam" without also meaning "an HTTP client" | `crates/synthia-provider/Cargo.toml` had no `[features]` beyond the inert `otel`; `reqwest` was a plain dependency |
| `WebFetchTool` was part of the default builtin set with no way to drop it | `build_default_tool_registry` registered it unconditionally |
| `synthia-core`'s `reqwest` feature was turned on unconditionally *by `synthia-provider`*, not by the adapters | `synthia-core = { workspace = true, features = ["tokio-util", "reqwest"] }` — which is why the R37 tree still had `reqwest` after the tool and provider work |
| `ProviderProfile::build_provider` and `build_provider_with_key` each carried their own copy of both adapter construction paths | four near-identical blocks in `profile.rs` |
| A build without an adapter had nothing to say about it | `create_provider` fell through to `"Unsupported provider type: openai"`, which blames the operator's config for a build decision |

## What landed

### A. `synthia-provider`: the adapters are features

`anthropic` and `openai` (both on by default) gate `pub mod anthropic`,
`pub mod openai`, `openai_streaming`, `streaming::anthropic`, and the
helpers that only the adapters call (`estimate_tokens` /
`count_message_tokens`, `pump_sse` / `CANCEL_GRACE` / `wait_cancel`,
`config::model_config_of`). Each feature also enables
`synthia-core/reqwest`, so the `From<reqwest::Error> for Error` impl
appears exactly when something can produce a `reqwest::Error`.

Naming an adapter a build does not have is a **typed runtime error**,
not a compile error and not a lie about the config:

- `config::adapter_not_compiled("openai")` →
  ``provider type `openai` is not compiled in: enable the `openai`
  feature of synthia-provider``;
- `config::no_adapter_compiled()` when neither is present,
  so `ProviderProfile::build_provider_with_key` reports the build
  configuration once instead of per profile kind.

Both `create_provider` and `build_provider` / `build_provider_with_key`
now route through one construction site per adapter
(`OpenAICompatibleProvider` in `openai_provider`, `AnthropicProvider` in
`anthropic_provider`), so the env-resolved key path and the explicit-key
path cannot drift — a duplication the cfg work exposed.

### B. `synthia-tool`: `web_fetch` is a feature

`web` (on by default) gates `builtin::web`, `WebFetchTool`, its
registration, and `reqwest`. Without it `build_default_tool_registry`
registers `read` / `write` / `shell` / `TodoWrite` — and the two tests
that pinned "exactly five tools" and the five render kinds now assert
**the compiled set** in both configurations instead of one hardcoded
list.

### C. The facade: transport is separate from vocabulary

| Feature | Means |
|---|---|
| `provider` | the `ModelProvider` trait, the wire types, the profiles — no HTTP |
| `provider-anthropic`, `provider-openai` | the shipped adapters |
| `tool` | the registry and the offline builtins |
| `tool-web` | `web_fetch` |

The default set enables all four, so `cargo add synthia` behaves as
before; the seven-feature subset in `MINIMAL.md` now means "no HTTP
client". Three new `compile_fail` doc proofs (both adapters and
`WebFetchTool` absent) plus three reachability tests pin gating from
both sides — 14 `compile_fail` proofs run in the MVP-subset doc test,
all green.

### D. The dependency promise got sharper

`make check-mvp-deps` now forbids `reqwest|hyper|rustls|h2|tower|webpki`
in addition to `opentelemetry|tonic|axum|rusqlite|sqlx`, with prefix
matching so `hyper-rustls`, `rustls-pki-types`, and `tower-http` are
caught. Verified in both directions: green on the seven-feature subset,
red when `tool-web` or `provider-openai` is added to it.

## Verification

| Check | Result |
|---|---|
| `cargo +nightly fmt --all` | clean |
| `cargo clippy --all-targets --all-features --tests --all -- -D warnings` | 0 warnings, 0 errors |
| `cargo clippy -p synthia-provider --no-default-features --all-targets -- -D warnings` | clean |
| `cargo clippy -p synthia-provider --no-default-features --features openai` (and `anthropic`) `--all-targets -- -D warnings` | clean, both |
| `cargo clippy -p synthia-tool --no-default-features --all-targets -- -D warnings` | clean |
| `cargo clippy -p synthia --no-default-features --features core,provider,context,tool,session,steering,agent --all-targets -- -D warnings` | clean |
| `cargo check -p <crate> --all-targets` for all 19 crates | 19/19 ok |
| **Facade seven-feature tree** | **158 → 124 distinct crates** |
| **`docs/examples/minimal-consumer` whole tree** | **165 → 125 distinct crates**; no `reqwest` / `hyper` / `rustls` / `h2` / `tower` / `webpki` (measured with `cargo tree`, and against a scratch worktree at `HEAD` for the before-number) |
| `cd docs/examples/minimal-consumer && cargo run` | `MVP-OK` — 2 model calls, 1 tool call, 12 structural events, `tool registry: 5 tools (4 builtins without web_fetch + echo)`; the program asserts `!registry.contains("web_fetch")` |
| `cd docs/examples/external-consumer && cargo run` | `CONSUMER-PROOF: OK` |
| `make check-mvp-deps` | `OK: … pulls no (opentelemetry|tonic|axum|rusqlite|sqlx|reqwest|hyper|rustls|h2|tower|webpki)` |
| `make check-mvp-deps MVP_FEATURES=…,tool-web` | `FAIL: … rustls-pki-types rustls-platform-verifier rustls-webpki tower tower-http tower-layer tower-service` + exit 1 |
| `make check-mvp-deps MVP_FEATURES=…,provider-openai` | `FAIL: … h2 hyper …` + exit 1 |
| `cargo test -p synthia --doc` (default features) | 12 passed / 0 failed — the assembly doctest plus the 11 opt-in-module absence proofs (the 3 new transport proofs are not emitted while their features are on) |
| `cargo test -p synthia --doc --no-default-features --features <7>` | **15 passed / 0 failed — 14 `compile_fail` gating proofs** (11 opt-in modules + 3 adapters/`web_fetch`) plus the assembly doctest |
| `cargo test -p synthia --tests` (default features) | 5 passed / 0 failed; the same command on the MVP subset runs the 1 test whose features are on |

Per-crate test totals (credential-free, default features):

| Crate | Passed | Crate | Passed |
|---|---|---|---|
| core | 104 | attachment | 15 |
| telemetry | 36 | mcp | 52 |
| provider | 744 | rag | 38 |
| context | 95 | scheduler | 14 |
| tool | 307 | macros | 31 |
| session | 137 | eval | 36 |
| steering | 74 | workflow | 65 |
| skill | 56 | test-support | 18 |
| agent | 306 | facade | 17 |
| | | server | 437 |

**2582 passed / 0 failed.** The narrowed configurations were run
separately and are green too: provider `--no-default-features` 353,
`--features openai` 589, `--features anthropic` 557; tool
`--no-default-features` 302.

## Deliberate decisions

- **`prometheus` stays in the MVP tree.** Removing it costs a fifth
  feature on `synthia-tool`, a facade feature, and a conditional around
  three metric calls, to save four net crates (`prometheus`,
  `lazy_static`, `fnv`, `memchr`). The tool counters are the contract
  `synthia-server`'s `/metrics` route serves; the trade is recorded here
  rather than paid silently.
- **`ProviderProfile::OpenAI` / `Anthropic` variants stay nameable
  without their adapters.** They are data (and the config round-trip
  depends on them); only *constructing* a provider without the adapter
  fails, with a message naming the feature.

## Deferred to R39 (recorded, not dropped)

- **`tokio = { features = ["full"] }` in `[workspace.dependencies]`** —
  every crate that inherits it compiles `process`, `signal`, `net`,
  `fs`, `io-std`, `parking_lot`, and `mio` whether or not it uses them
  (`mio` + `socket2` are the last network-ish crates left in the MVP
  tree). Narrowing the shared entry plus per-crate additions is
  mechanical but touches every manifest.
- **`synthia-core`'s `tokio-util` bridge is unconditional in
  `synthia-provider`** (needed by the adapter streaming paths); it could
  follow the adapter features as well.
- **`synthia-server`'s `middleware/trace_context.rs` still reimplements
  `synthia_telemetry::propagation`** (carried over from R37's list).
- **A runtime seam** (`Spawn` / `Timer` traits in `synthia-core`) so
  `synthia-agent` / `synthia-session` / `synthia-tool` stop linking
  tokio for spawn, timers, fs, and process — the one remaining sense in
  which the composition is not runtime-neutral.
