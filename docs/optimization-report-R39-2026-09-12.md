# R39 Absorption Results — 2026-09-12

Predecessor: [`optimization-report-R38-2026-09-12.md`](optimization-report-R38-2026-09-12.md)
(`59215394`).

R37 and R38 cut what the minimal agent *compiles*. R39 attacked what
the loop *requires*: the framework's headline promise — "the executor
is the consumer's choice" — did not hold for the one place that needs
an executor at all.

## What the audit found

| Finding | Evidence |
|---|---|
| **The agent loop hard-coded `tokio::spawn`.** [`Agent::run`] detached one task per turn (so a caller that stops polling the event stream does not cancel the run) with a direct `tokio::spawn`, which compiles only against tokio and *panics at runtime* ("there is no reactor running") under any other executor | `crates/synthia-agent/src/agent/re_act.rs`, the single production `tokio::spawn` in the crate |
| **Tool dispatch hard-coded it too**, once per tool call plus a semaphore | `ToolRegistry::run_stream`, `crates/synthia-tool/src/registry.rs` |
| **`synthia --no-default-features` did not compile**, contradicting the prelude's own docs ("the prelude is empty and `use synthia::prelude::*;` still compiles") | the prelude re-exported `crate::core::*` unconditionally while `pub mod core` is `#[cfg(feature = "core")]`; the configuration had never been built |
| A public field in the test-support crate named a tokio type, pushing a runtime into every consumer's test code | `synthia_test_support::FakeTool::call_count: Arc<tokio::sync::Mutex<usize>>` |
| The rest of the runtime coupling was undocumented, so a consumer could not tell "the loop needs tokio" from "the built-in tools need tokio" | no single place stated the boundary |

## What landed

### A. `synthia_core::spawn` — detached work, no runtime

```rust
pub type BoxFuture<T = ()> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

pub trait Spawner: Send + Sync + 'static {
    fn spawn(&self, task: BoxFuture<()>);
    fn spawn_blocking(&self, f: Box<dyn FnOnce() + Send + 'static>) { … }
}
```

Object-safe (`Arc<dyn Spawner>` is what the crates store), no generics
in the blocking variant (the closure is erased, so the trait stays
usable as a trait object), and **std-only**: the module's own tests
implement a spawner with `std::task::Waker::noop()` and prove the trait
needs nothing from an executor.

### B. The agent loop and tool dispatch use it

- `ReActAgent::with_spawner` / `AgentBuilder::spawner` /
  `ToolRegistry::with_spawner`, each defaulting to a private
  tokio-backed spawner (both crates already link tokio for their own
  channels and timers, so the default costs nothing and behaviour is
  unchanged).
- `ToolRegistry`'s `Clone` propagates the spawner, so a registry cloned
  for a child run keeps the consumer's executor.

### C. The proof: `cargo run --example runtime_agnostic -p synthia-agent`

A full turn — scripted provider, one tool call, 11 events — on an
executor that is *not* tokio:

```
spawner          : std::thread + futures::executor::block_on
events observed  : 11
model calls      : 2
tool calls       : 1
answer           : the loop ran without tokio

RUNTIME-AGNOSTIC: OK
```

The spawner is ten lines (`std::thread::spawn` + `futures::executor::block_on`);
`main` is a plain `fn main` with every await inside `block_on`; the
provider is scripted and the tool is consumer-owned. Had any part of
the loop still called `tokio::spawn` / `tokio::time`, this program
would have panicked instead of printing its proof line.

### D. The boundary is written down

`synthia_core::spawn`'s module docs, the `synthia-agent` crate docs,
and the README now state the same three-part contract:

| Runtime-neutral | tokio-bound plugin | needs a reactor today |
|---|---|---|
| the loop, the event stream, cancellation, clock/id, the context managers, steering, dispatch | builtin tools (`tokio::fs`, `tokio::process`), provider HTTP adapters (`reqwest`), the JSONL session sink | provider retry backoff, the streaming idle watchdog |

A consumer on another runtime supplies their own `Tool` /
`ModelProvider` / `SessionSink` and keeps everything in the first
column — which is what the example does.

### E. Two smaller fixes found on the way

- **`synthia --no-default-features` compiles again** (the prelude block
  is now gated like everything else), and `make check-mvp-deps` asserts
  it alongside the dependency check, so the documented "empty, not
  broken" promise is now compiled on every gate run.
- **`FakeTool::call_count` is a `parking_lot::Mutex`**, not
  `tokio::sync::Mutex`: the field is public, so the tokio type was part
  of test-support's API. This was the last public signature in the
  workspace naming a runtime type (`CleanupTask`'s `JoinHandle` is a
  private field, not a signature).
- `synthia::prelude` gained `Spawner` + `SharedSpawner`.

## Verification

| Check | Result |
|---|---|
| `cargo +nightly fmt --all` / `--check` | clean |
| `cargo clippy --all-targets --all-features --tests --all -- -D warnings` | 0 warnings, 0 errors |
| `cargo clippy -p synthia-provider` (no features / `openai` / `anthropic`) | clean |
| `cargo clippy -p synthia-tool --no-default-features` | clean |
| `cargo clippy -p synthia --no-default-features --features <7>` | clean |
| `cargo check -p synthia --no-default-features` | clean (the bug this round fixed) |
| **Every example, 30/30** | exit 0, including `runtime_agnostic` → `RUNTIME-AGNOSTIC: OK` |
| `cd docs/examples/minimal-consumer && cargo run` | `MVP-OK` |
| `cd docs/examples/external-consumer && cargo run` | `CONSUMER-PROOF: OK` |
| `make check-mvp-deps` | `OK: synthia --no-default-features compiles` + `OK: … pulls no (opentelemetry\|tonic\|axum\|rusqlite\|sqlx\|reqwest\|hyper\|rustls\|h2\|tower\|webpki)` |

Per-crate test totals (credential-free, default features):

| Crate | Passed | Crate | Passed |
|---|---|---|---|
| core | 107 | attachment | 15 |
| telemetry | 36 | mcp | 52 |
| provider | 744 | rag | 38 |
| context | 95 | scheduler | 14 |
| tool | 307 | macros | 31 |
| session | 137 | eval | 36 |
| steering | 74 | workflow | 65 |
| skill | 56 | test-support | 18 |
| agent | 306 | facade | 17 |
| | | server | 437 |

**2585 passed / 0 failed** (R38: 2582; the +3 are the `spawn` module's
tests).

## Deferred to R40 (recorded, not dropped)

- **Timers are still tokio.** Provider retry backoff and the streaming
  idle watchdog call `tokio::time::sleep` / `timeout` / `select!`. A
  `Timer` seam alongside `Spawner` (or a reactor-independent timer
  crate) would finish the job for the provider path; the HTTP adapters
  keep `reqwest` regardless.
- **The builtin tools are tokio-bound** (`tokio::fs`,
  `tokio::process`). Making `read` / `write` / `shell` runtime-neutral
  means an async-fs/process abstraction — a much larger surface than
  `Spawner`, and the tools are replaceable plugins today.
- **`tokio = { features = ["full"] }`** at the workspace level is still
  inherited by every crate (carried over from R38): narrowing it is
  mechanical but touches every manifest.
- **`synthia-server`'s `middleware/trace_context.rs`** still
  reimplements `synthia_telemetry::propagation` (carried over from
  R37/R38).
