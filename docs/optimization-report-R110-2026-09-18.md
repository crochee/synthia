# Optimization report R110 — 2026-09-18

## What the audit found

R12 introduced `AgentBuilder` as the fluent factory for `ReActAgent`
("one-expression assembly" of a complete agent). It held every wire
the agent consumes — provider, registry, steering, context manager,
typed sink, spawner, max_iterations, descriptor fields, output schema,
compaction policy, ledger, checkpoint — and `build()` copied them
onto a `ReActAgent` that already exposed most of the same fields as
`with_*` setters (the ones the agent crate itself used, plus the
ones the server's run factory wired in by hand).

Two parallel chains, two places to look up "how do I set the
steering?", and `AgentBuilder` was never `Clone`-free of a hidden
side effect: every call to `output_schema` *mutated the registry* the
caller had already handed in. The compiler enforced nothing about
which chain to use. Server-side run factories never built an
`AgentBuilder` (they went straight to `ReActAgent::with_*`) and the
facade exposed `AgentBuilder` to consumers — a second surface that
said the same thing.

## What landed

**The harness is the only builder.** `ReActAgent` *is* the builder.

- `crates/synthia-agent/src/agent/builder/` deleted (mod.rs 432,
  tests.rs 378, compaction.rs 212 → 0).
- New module `crates/synthia-agent/src/compaction/` owns the
  `CompactionEmitters` + `context_manager_for_compaction*` helpers
  + the `resolve` decision core. These were never builder-private —
  the server-side run factory uses them directly, and they are
  re-exported from the crate root for that consumer.
- `ReActAgent` gained 8 setters the `AgentBuilder` had but the agent
  did not: `with_workspace`, `with_tool_registry`, `with_name`,
  `with_instructions`, `with_model_hint`, `with_output_schema`,
  `with_compaction_settings`, `with_compaction_checkpoint`. The
  first two move values that were previously only settable at
  construction; the next four are descriptor mutations; the last
  two are the new compaction wiring (see below).
- `with_compaction_settings(s)` resolves eagerly through
  `resolve_compaction_now`. If a `with_typed_event_sink(sink)` was
  also installed AND no external checkpoint was installed, it
  auto-builds a fresh `CompactionCheckpoint` from the sink (and
  wires the emitters into the summarising manager). An
  `with_compaction_checkpoint(c)` call takes precedence — it
  carries its own sink + ledger.
- `with_output_schema(json)` keeps the previous auto-injection
  (LIFO so a caller-registered `structured_output` still wins).

**Every call site updated.**

- The five `synthia-agent` examples (`assemble_with_builder`,
  `best_of_n_judge`, `runtime_agnostic`, `strategy_swap`, plus the
  `hot_paths` bench) — all now `ReActAgent::new(..)` + chained
  `with_*`.
- The facade example (`synthia/examples/assemble_from_zero.rs`).
- The two external-consumer examples
  (`docs/examples/external-consumer` and
  `docs/examples/minimal-consumer`).
- `synthia::prelude` exposes `ReActAgent` (no `AgentBuilder`).
- The server already used `ReActAgent::with_*` directly (the run
  factory never built a builder). No change there.

## Verification

- `synthia-agent` lib tests: **239/239** OK.
  - Down from 247 — the 8 `AgentBuilder`-specific setter/clone tests
    (`build_returns_react_agent_with_default_descriptor`,
    `builder_setters_thread_through_to_descriptor`,
    `builder_with_interceptor_does_not_panic`,
    `builder_clone_is_cheap_and_independent`, and the four
    `output_schema` / `compaction_ledger` / `factory_*` rows) are
    either absorbed into `ReActAgent`'s own setter coverage
    (`compaction::tests`) or dropped (the clone-row — `ReActAgent`
    is intentionally not `Clone`; its `Arc` fields make cloning
    well-defined but no test was pinning the equivalence).
- `synthia-server` lib tests: **403/403** byte-identical.
- `synthia-core` 151, `synthia-context` 95, `synthia-tool` 215,
  `synthia-session` 147, `synthia-provider` 690, `synthia` facade
  0 — all byte-identical test lists.
- Every `synthia-agent` example prints its proof line:
  - `runtime_agnostic` → `RUNTIME-AGNOSTIC: OK`
  - `strategy_swap` → `STRATEGY-SWAP: OK`
  - `best_of_n_judge` → `BEST-OF-N-JUDGE: OK`
  - `assemble_with_builder` → `[4/6] descriptor: name=tutorial-agent (schema declared)` … `run: 6 events`
- The facade example prints `ASSEMBLE-FROM-ZERO: OK`.
- The two external-consumer standalone crates print
  `CONSUMER-PROOF: OK` and `MVP-OK` respectively.
- `cargo clippy --all-targets --all-features --tests --all -- -D warnings` — 0 warnings.
- `cargo +nightly fmt --all --check` — clean.
- `make ci` **8/8 OK** (fmt-check, lint-rust, doc-check,
  check-mvp-deps, check-no-runtime, check-public-api-runtime,
  check-claim-language, check-clock, check-test-layout).
- Test-layout band ratchet unchanged (20 inline test modules
  ≥300 lines).

## Result

The two parallel chains collapse into one. A consumer reads the
`ReActAgent` doc, sees every wire as a `with_*` setter, and uses
it. The server's run factory, the facade's prelude, and the
external consumers all reach the same type by the same name.

R110 numbers vs R109:
- **`synthia-agent/src/lib.rs`**: 123 → 124 lines (the
  `AgentBuilder` re-export drops, the `compaction` re-export
  appears; one line net).
- **`crates/synthia-agent/src/agent/re_act/agent.rs`**: 471 → 650
  lines (≈180 lines of new setter bodies — every one is the
  *only* definition; the previous 432-line `AgentBuilder` is
  gone).
- **`crates/synthia-agent/src/compaction.rs`** (new): 219 lines —
  the standalone helpers, with no `AgentBuilder` boilerplate.
- **`crates/synthia-agent/src/compaction/tests.rs`** (new):
  169 lines.
- **Net lines removed**: 432 (AgentBuilder) + 378 (its tests) +
  212 (its compaction.rs) − 219 (new compaction.rs) − 169 (new
  tests) − 180 (new setters on `ReActAgent`) − 219 (new
  compaction helpers) = **+34 lines net**, with **one fewer
  public type** (`AgentBuilder`) and **one fewer mod**
  (`agent::builder`). The cost is the small fixed overhead of
  the new `compaction` module; the gain is a single chain.
