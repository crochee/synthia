# Optimization report R101 — 2026-09-17

## What the audit found

R100 closed `builder.rs`. The last multi-concern file flagged in
R99's deferred list was `agent/team.rs` (900 lines): three
**independent** compositions sharing one file —

| Lines | Composition |
|---|---|
| 67–143 | `BoundAgent` + `FnAgent` — the agent boundary teams compose over |
| 145–292 | `Verdict` / `Verifier` / `VerificationChain` / `ChainOutcome` — generate-verify-retry |
| 294–609 | `Turn` / `TerminationCondition` (+ `MaxRounds` / `Marker`) / `RoundRobinGroupChat` / `GroupChatResult` / `StopReason` |
| 610–900 | tests (290 lines) |

Unlike the loop splits (R97/R98) these are not phases of one
orchestrator — they are three separately usable pieces that merely
ship together, so the treatment is per-type modules, not named
phases.

## What landed

Pure code motion, public paths unchanged. `team.rs` became a
`team/` directory:

```
team/mod.rs         90 lines   module docs + re-exports (the public surface)
team/bound.rs       91 lines   BoundAgent + FnAgent
team/verification.rs 159 lines Verdict + Verifier + VerificationChain + ChainOutcome
team/group_chat.rs  324 lines Turn + TerminationCondition + RoundRobinGroupChat + results
team/tests.rs       292 lines behaviour tests for all three
```

`mod.rs` re-exports every public name (`BoundAgent`, `FnAgent`,
`Verdict`, `Verifier`, `VerificationChain`, `ChainOutcome`,
`SEED_SPEAKER`, `Turn`, `TerminationCondition`,
`MaxRoundsTermination`, `MarkerTermination`,
`RoundRobinGroupChat`, `GroupChatResult`, `StopReason`), so the
`agent/mod.rs` re-export block and every consumer
(`agent_teams` example, `cot.rs` doc link,
`synthia-delegation`) resolves unchanged.

Two doc-link fixes rode along: `BoundAgent`'s doc referenced
`super::Agent` / `super::ReActAgent` — from `team/bound.rs`,
`super` is now `team`, so the links were rewritten to
`crate::agent::Agent` / `crate::agent::ReActAgent`.

## Verification

- `make ci` **7/7 green** (fmt-check, clippy `-D warnings`,
  rustdoc `-D warnings`, MVP-deps, runtime-free,
  public-API-runtime, claim-language, clock).
- `cargo test -p synthia-agent --lib` **247/247** — baseline
  identical; the 10 team tests run at `agent::team::tests::*`
  unchanged.
- `cargo test -p synthia-server --lib` **403/403**;
  `synthia-delegation` **66/66**.
- `cargo run -p synthia-agent --example agent_teams` →
  `AGENT-TEAMS: OK` (drives all three compositions end to end).
- `cargo +nightly fmt --all` clean.

## Result

`synthia-agent/src/agent/` now has no file above 500 lines except
`best_of_n.rs` (676 — one strategy + its 320-line test block) and
`re_act/loop_/drive.rs` (603 — the R98 orchestrator whose size is
its named phases). Every multi-concern file flagged since R95 has
been split; the "one concern per file" property now holds across
the loop (`loop_/`), the strategy seam (`strategy/`), the builder
(`builder/`), and the team compositions (`team/`).
