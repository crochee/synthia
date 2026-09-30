# R49 Results — 2026-09-12

Predecessor: [`optimization-report-R48-2026-09-12.md`](optimization-report-R48-2026-09-12.md)
(`bdf2addc`).

Every capability this framework offers is a swappable trait — provider,
tools, context manager, steering, sink, cancel token, executor,
retriever, MCP transport, attachment backend. **The reasoning loop was
the exception**: `ReActAgent` owned both the reasoning and the plumbing,
so a different paradigm meant reimplementing the plumbing.

## What the audit found

| Finding | Evidence |
|---|---|
| **No strategy seam.** The reference project this repo draws on most for agent shape (`traitclaw`) has an `AgentStrategy` + `AgentRuntime` pair with three shipped strategies; `synthia` had none — `grep 'pub trait Strategy'` over `crates/` returned nothing | `crates/traitclaw-core/src/traits/strategy.rs` vs the synthia tree |
| **The loop and the plumbing were one unit.** `ReActAgent::run` cloned 20 fields into a private `ReActLoop`; anything else that wanted to reason had to rebuild that clone, the event channel, the typed sink and the streaming sink by hand | `crates/synthia-agent/src/agent/re_act.rs` `impl Agent for ReActAgent` |
| **`SEAMS.md` documented the gap honestly but only as advice**: "implement `Agent` for another paradigm and keep every seam above" — which is true and useless without a runtime to keep them in | `SEAMS.md` §1, before this round |
| The clock ratchet counted the forbidden string inside doc comments, so a doc that *explains* the rule could trip the gate | `make check-clock` counted 26 with the new docs vs a baseline of 25 |

## What landed

### A. `ReasoningStrategy` — the loop behind a seam

```rust
#[async_trait]
pub trait ReasoningStrategy: Send + Sync + 'static {
    fn name(&self) -> &str;
    async fn run(&self, runtime: AgentRuntime, input: AgentInput, sink: EventSink);
}
```

- **`AgentRuntime`** — every assembled piece in one place: provider,
  tool registry, tool-surface policy, workspace, descriptor, prompt
  context, context manager, peer registry, steering, max iterations,
  typed sink, inbox, clock, spawner, cancel token, subagent depth. All
  public, so a consumer can construct one by hand; `ReActAgent` builds
  it (`runtime_for_run`) and `AgentBuilder` fills it.
- **`EventSink`** — clone it into whatever concurrent work the strategy
  spawns; `emit`, `emit_typed`, `text_delta`, `is_closed`.
- **`ReActStrategy`** — the default, and the reference implementation of
  the seam. `ReActLoop` keeps its flat field layout (the runtime is
  destructured once in `from_runtime`), so no method inside the loop
  changed and the 300+ ReAct tests still describe it.
- **`AgentBuilder::strategy(..)` / `ReActAgent::with_strategy(..)`** —
  the assembly-level swap. `Agent::run` is now: build the runtime,
  build the sink, hand both to the strategy.

### B. One method, streaming-first (an improvement on the reference)

traitclaw's shape is `execute()` (a final output) plus an optional
`stream()` that **defaults to an error**, which makes streaming a
second-class capability a strategy must opt into. This seam has one
method and the event stream is the contract: a strategy publishes, the
caller's stream carries everything, and failure travels as
`SystemEvent::SessionEnded { reason }` — the same convention
`Agent::run` already documented. Consequences: no dual output shape, no
"streaming unsupported" path, and the engine cannot drift from the
stream.

### C. `ChainOfThoughtStrategy` — the second paradigm, as proof

One streaming completion with no tools, a step-bounded instruction
(`Step N:` … `Answer:`), the runtime's context manager honoured before
the pass, cancel checked before spending, and a public
`parse_steps()` that recovers the labelled chain from the answer. It
also warns (rather than silently dropping) if a model emits a tool call
anyway.

It is deliberately *simple* — but it is a complete paradigm that shares
every piece of plumbing, which is exactly what the seam needed to
prove. Multi-pass self-critique or verification is a different
strategy (and `VerificationChain` already exists for composing one).

### D. `cargo run --example strategy_swap -p synthia-agent`

```
[1/2] ReActStrategy — tools are offered, the loop uses them
      strategy     : react        tools offered: 1        tool calls: 1
[2/2] ChainOfThoughtStrategy — no tools, step-by-step prompt
      strategy     : chain-of-thought  tools offered: 0   steps parsed: ["read the request.", "answer directly."]

STRATEGY-SWAP: OK
```

The assertions read the *provider's recorded requests*, so the proof is
the wire traffic, not the printed text: ReAct advertises the registry's
tool and runs it once; Chain-of-Thought advertises none and runs none,
with the same registry assembled in the same builder.

### E. The clock gate counts code, not prose

`make check-clock` now ignores comment lines (a line whose first
characters are `//`), which is what it always meant: doc comments
legitimately name the call the rule forbids. Baseline is the 18
in-crate `#[cfg(test)]` uses; `make clock-audit` prints only real call
sites. This surfaced while adding the docs above — the ratchet was
counting its own explanation.

## Verification

| Check | Result |
|---|---|
| `cargo +nightly fmt --all` / `--check` | clean |
| `cargo clippy --all-targets --all-features --tests --all -- -D warnings` | 0 warnings, 0 errors |
| `cargo test -p synthia-agent` | **317 passed / 0 failed** (310 before: +4 seam tests, +7 Chain-of-Thought tests) |
| Full per-crate sweep (19 crates) | **2600 passed / 0 failed** — core 108, telemetry 36, provider 744, context 95 (103 with `sqlite`), tool 309, session 137, steering 74, skill 56, **agent 317**, attachment 16, mcp 52, rag 38, scheduler 14, macros 31, eval 36, workflow 65, test-support 18, facade 17, server 437 |
| `cargo run -p synthia-agent --example strategy_swap` | `STRATEGY-SWAP: OK` (proof above) |
| `cargo run -p synthia-agent --example runtime_agnostic` | `RUNTIME-AGNOSTIC: OK` (unchanged) |
| `make ci` | green — including the corrected `check-clock` (18 code calls) |
| `make examples` | every example plus both consumer crates: `MVP-OK`, `CONSUMER-PROOF: OK` |

Behaviour is unchanged for every existing user: `ReActAgent`'s default
strategy is `ReActStrategy`, which is the loop that was already there.

## Deferred to R50 (recorded, not dropped)

- **A strategy reachable from deployment config.** The seam is
  code-level (`AgentBuilder::strategy`); a `[agents.<name>] strategy =
  "chain-of-thought"` server knob would make it operator-visible. Small,
  but it belongs with the other server config work rather than bolted on
  here.
- **A third paradigm.** `synthia-workflow`'s `Step::BestOf` /
  MCTS-style branching covers tree search declaratively; a
  `ReasoningStrategy` version (branch → score → select inside one
  session) would be the natural third, and the seam now makes it a
  self-contained addition.
- Carried: the builtin tools' runtime seam (R40 — declined by design,
  restated in `SEAMS.md`), the `Timer` seam (R39), `cargo-deny` in CI
  (R41), a doctest gate over `SEAMS.md` (R47), a benchmark baseline in
  CI (R48).
