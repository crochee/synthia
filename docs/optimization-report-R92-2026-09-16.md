# Optimization report R92 — 2026-09-16

## What the audit found

R91 closed the server's boot orchestrator. The fresh
workspace scan (`clippy --workspace -W
cognitive_complexity`) put the harness core next in line —
the `re_act/loop_/` files held the worst *production-code*
scores in the workspace:

| Function | File | Score |
|---|---|---|
| `execute_tools` | `loop_/dispatch.rs` | 62/20 |
| `execute_tool_inner` | `loop_/dispatch.rs` | 42/20 |
| `dispatch_tool_call` | `loop_/dispatch.rs` | 38/20 |
| `sample_once` | `loop_/steps.rs` | 33/20 |
| `prepare` | `loop_/steps.rs` | 24/20 |

These are exactly the files the user's directive names
("mvp核心循环只要harness，简单明了 … harness自身也要分好
层次和布局"). The 144/20 `run_controller_loop` in the server
is larger but is app wiring, not the lib.

One measurement lesson recorded mid-round: `cargo clippy -p
<crate>` can report zero where the `--workspace` union
feature graph still reports violations (feature-gated
branches differ), and cached runs hide fresh results — the
workspace-wide scan after `cargo clean -p` is the
authoritative check.

## What landed

Every split keeps behaviour byte-identical (256/256
synthia-agent tests unchanged); only the shape moves.

### `execute_tools` → four phase helpers

The doc comment already promised "0. record → 1. bucket →
2. parallel → 3. sequential → 4. commit"; the body now is
that list:

- `bucket_by_execution_mode` — also owns the concurrency
  clamp and the "bucketed" `info!` (they report bucket
  sizes; they belong with the bucketing).
- `run_parallel_bucket` — semaphore + `join_all`.
- `run_sequential_bucket` — loop control only; each call
  delegated to `run_one_sequential`, which classifies its
  outcome as a `SequentialStep { Continue, Cancelled,
  Aborted }` enum.
- `commit_all_results` — synthetic-error fill, steering
  output-transform + hint append, wire commit, summary log.

### `execute_tool_inner` → the steering seams as named fns

- `pre_execution_veto` — R58 restriction + the
  `BeforeToolExecute` votes (the double-`Result` flattening
  lives in `hook_before_verdict`); a block fans out to
  `notify_error_hooks`.
- `apply_guard_pipeline` — returns `Verdict<ToolUse>`
  (`Allowed` / `Denied`). A private enum, not
  `Result<_, ToolOutput>`: the denial is a value the caller
  commits, not an error it handles — and `ToolOutput` is
  large enough to trip `result_large_err`.
- `notify_after_hooks` — the `AfterToolExecute` fan-out.

### `dispatch_tool_call` → three steps

`try_interceptor` (claimed synthetic tools) →
`lookup_tool_entry` (touched-file note + registry
resolution, also `Verdict`) → `drain_tool_stream`
(generic over `S: Stream<Item = StreamOutput> + Unpin`;
forwards Progress chunks as `ToolProgress` events,
disambiguating `next()` between the tokio_stream and
futures trait imports).

### `sample_once` → pipeline of three

- `notify_provider_start_hooks`
- `stream_completion` — cancel-aware wiring; owns the
  `ChunkState`'s lifetime and finalizes the outcome
  (stop-reason stamp) inside; returns `(resp, outcome,
  elapsed)`.
- `observe_provider_success` — usage fold, typed event,
  tracker, `OnProviderEnd` with the measured elapsed.

The provider-error arm reuses `notify_error_hooks` —
promoted from a dispatch-local fn to
`pub(in crate::agent::re_act::loop_)` — so the veto path
and the stream-error path run the identical fan-out.

### `prepare` → `assemble_system_prompt`

The manifest-resolution match, the assembly, and the
"assembled" debug log fold into one helper; `prepare`
keeps the two boundary logs and the message-vector
construction. (Three `tracing` macro expansions cost real
complexity points — each expands to a level-check branch —
so pairing each log with the phase it reports is also the
cheapest split.)

## Result

```
$ cargo clippy --workspace --all-targets --all-features --tests \
    -- -W clippy::cognitive_complexity | grep synthia-agent
(no output — zero violations in the harness crate)
```

Before R92: five functions over threshold (62/42/38/33/24).
After: none.

Remaining workspace hits (production code), for later
rounds: `run_controller_loop` 144/20
(`synthia-server/session/controller.rs` — R93 candidate),
`synthia-provider` streaming 39/20 / 23/20,
`synthia-context/summarizing.rs` 25/20,
`synthia-session/log_surface.rs` 25/20, plus several
21-26s scattered across server crates.

## Verification

- `make ci` **7/7 green** (fmt-check, clippy `-D warnings`,
  rustdoc `-D warnings`, MVP deps, runtime-free, public-API
  runtime ban, claim-language, clock).
- `make test-unit` **2446/2446**; `synthia-agent` 256/256
  unchanged — assertions byte-identical.
- `cargo +nightly fmt --all` clean.
- Harness behaviour proofs unchanged: the same 256 tests
  cover the loop lifecycle, parallel dispatch, steering
  wiring, tool restriction, and the run inbox.

## Deferred

R93 candidate: `run_controller_loop` (144/20) — the server
session controller's op-loop, the single largest function
left in the workspace. Same treatment: named phases
(dequeue → dispatch op → idle/shutdown control), each in
its own helper.