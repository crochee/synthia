# Optimization report R99 — 2026-09-17

## What the audit found

R97/R98 closed the harness's dispatch seam and main-loop body.
The follow-up audit turned to the **strategy seam** — the module
that makes the reasoning loop a swappable part
(`AgentBuilder::strategy(...)`, `strategy::from_name(...)`).
`synthia-agent/agent/strategy.rs` was 556 lines holding **three
distinct concerns** side by side:

| Lines | Concern |
|---|---|
| 60–153 | `AgentRuntime` — the 22-field assembled-pieces struct a strategy receives (plus `model_config` / `system_prompt` derivations, `Debug`) |
| 197–274 | `EventSink` — the strategy-facing publish channel (emit / emit_typed / is_closed / text_delta / into_parts, `Debug`) |
| 276–362 | The `ReasoningStrategy` trait + `KNOWN_STRATEGY_NAMES` + the `from_name` config resolver |
| 364–556 | tests (190 lines) |

Any reader opening the file to answer "what does a strategy
receive?", "how does it publish?", or "how do I name one from
config?" had to skip past the other two questions' code. The
"every concern in its own file" property R97/R98 established in
the loop had not yet been applied to the seam that sits **above**
the loop.

## What landed

Pure code motion, public paths preserved. `strategy.rs` became a
`strategy/` directory with four single-concern files:

```
strategy/mod.rs       157 lines   seam docs + ReasoningStrategy trait + KNOWN_STRATEGY_NAMES + from_name
strategy/runtime.rs   172 lines   AgentRuntime struct + model_config/system_prompt + Debug + default_for_test
strategy/sink.rs       92 lines   EventSink struct + publish primitives + Debug
strategy/tests.rs     205 lines   seam-level tests (foreign strategy, name resolver, sink closed-state)
```

### Public surface — unchanged

Every existing path still resolves:

- `synthia_agent::agent::strategy::{AgentRuntime, EventSink, ReasoningStrategy, KNOWN_STRATEGY_NAMES, from_name}` — the module re-exports its children.
- `crate::agent::strategy::default_for_test` — still `pub(crate)` + `#[cfg(test)]`, now re-exported as `pub(crate) use` so `cot.rs` / `best_of_n.rs` / `strategy/tests.rs` keep compiling unchanged.
- `synthia_agent::{AgentRuntime, EventSink, ReasoningStrategy, KNOWN_STRATEGY_NAMES, from_name}` — the crate-root re-exports in `agent/mod.rs` were never touched.

No callsite outside the `strategy/` directory changed.

### Why `default_for_test` moved to `runtime.rs`

It builds an `AgentRuntime` literal — the concern it belongs to.
Keeping it next to the struct means a new field on `AgentRuntime`
is a one-file edit (struct + factory), not a two-file hunt.

## Verification

- `make ci` **7/7 green** (fmt-check, clippy `-D warnings` on the
  whole workspace, rustdoc `-D warnings`, MVP-deps,
  runtime-free, public-API-runtime, claim-language, clock
  baseline).
- `cargo test -p synthia-agent --lib` **247/247** — identical to
  the pre-change baseline at `b1425516` (verified via
  `git stash` round-trip; the earlier 256→247 delta came from the
  descriptor move to `synthia-core`, not this round). Pure code
  motion, zero test churn.
- `cargo test -p synthia-delegation --lib` **66/66**.
- `cargo test -p synthia-core --lib` **151/151** (the descriptor
  tests' new home).
- Strategy consumers exercised end to end:
  `cargo run -p synthia-agent --example strategy_swap` →
  `STRATEGY-SWAP: OK` (runs all three shipped strategies against
  one runtime).
- `cargo run -p synthia-agent --example runtime_agnostic` →
  `RUNTIME-AGNOSTIC: OK` (the non-tokio executor proof, which
  drives the seam directly).
- `cargo clippy -p synthia-agent --all-targets --all-features
  --tests -- -D warnings -W clippy::cognitive_complexity` —
  zero violations.
- `cargo +nightly fmt --all` clean.

## Result

The strategy seam now has the same per-concern layout as the
loop below it: the trait is the contract, the runtime and the
sink are the two data structures it moves, and each lives in the
file a reader would guess from its name. The biggest file in
`synthia-agent/src/agent/` that was still multi-concern is gone;
what remains above 500 lines (`builder.rs` 991, `team.rs` 900,
`best_of_n.rs` 676) is single-concern by its own right (one
factory, one team-composition module, one strategy) and would
need a different treatment than file-splitting.

## Deferred

- `builder.rs` (991 lines) — the `AgentBuilder` factory + the
  compaction-emitter wiring + its own test block. The natural
  next round: extract the compaction half
  (`CompactionEmitters`, `context_manager_for_compaction*`,
  `resolve_context_manager`) into a `builder/compaction.rs`
  sibling, mirroring this round's shape.
- `team.rs` (900 lines) — three independent compositions
  (`BoundAgent`, `VerificationChain`, `RoundRobinGroupChat`)
  sharing one file; a future round could give each its own
  module under a `team/` directory.
