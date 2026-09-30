# R50 Results — 2026-09-12

Predecessor: [`optimization-report-R49-2026-09-12.md`](optimization-report-R49-2026-09-12.md)
(`9725bf23`).

R49 turned the reasoning loop into a part. R50 makes it a *useful* one:
a second shipped paradigm that exercises the seam's concurrency half,
and the deployment hop so the choice is not compiled in.

## What the audit found

| Finding | Evidence |
|---|---|
| **The seam had one and a half paradigms.** `ReActStrategy` (a loop with tools) and `ChainOfThoughtStrategy` (one prompt, no tools) are both *sequential single-pass* shapes; nothing in the crate showed a strategy spawning work, so the seam's concurrency story — `AgentRuntime::spawner`, `EventSink` cloned into tasks, cancellation reaching in-flight calls — was asserted in docs and untested by any implementation | `crates/synthia-agent/src/agent/{re_act,cot}.rs`, both sequential |
| **Best-of-N was only declarative.** `synthia-workflow`'s `Step::BestOf` samples N candidates, but that is a *multi-agent* runtime with a journal and a host; there was no single-session "sample N, score, pick" loop, which is the form self-consistency and green-patch selection actually take | `crates/synthia-workflow/src/` `BestOfStep` |
| **The strategy was invisible to operators.** R49's seam was `AgentBuilder::strategy(...)` — code. `agents.<name>.max_steps` and `agents.<name>.compaction` are config; `strategy` was not, so a deployment could not switch loops without a rebuild | R49 report §Deferred, quoted verbatim |
| The provider's wire→agent conversion (`completion_to_sampling`) lived at `synthia_provider::traits::completion_to_sampling`, so a consumer that had a `CompletionResponse` and wanted to publish it had to know the module layout | `crates/synthia-provider/src/traits.rs` |

## What landed

### A. `BestOfNStrategy` — sample N, score, publish the winner

```rust
pub trait CandidateScorer: Send + Sync + 'static {
    fn score(&self, candidate: &str) -> f64;   // higher wins
    fn name(&self) -> &str { "candidate-scorer" }
}

let agent = AgentBuilder::new(provider)
    .strategy(Arc::new(BestOfNStrategy::new(5, Arc::new(PrefersShorter))))
    .build();
```

- **Concurrency through the seam, not around it.** Candidates are
  spawned with `runtime.spawner` (the deployment's executor) and
  collected over `futures::channel::mpsc`, so the strategy names no
  runtime and a consumer on `std::thread` + `futures::executor` gets
  their own executor used — the same property
  `example runtime_agnostic` proves for the loop itself.
- **Alternatives stay off-stream.** Fanning five answers at a chat UI is
  noise, so candidates are generated on a side channel; what reaches the
  run's stream is the winner (`AgentEvent::Model` + `ModelDone`), one
  `Progress` event per candidate ("candidate 2/3 (index 0) scored 1.000
  via requires-marker"), and a selection event with the elapsed time.
- **Empty answers are failures, not candidates** — an empty string
  scores as a failure and cannot win.
- **Ties go to the lowest index** (`sort_by` on (score desc, index asc)),
  so the same script produces the same winner.
- **Cancellation reaches in-flight samples**: each candidate calls
  `complete_with_stream(request, Some(cancel), |_| {})` — the deltas are
  discarded (they are alternatives), but the token is honoured. A
  pre-cancelled run issues zero requests.
- **No tools advertised**, deliberately (`tools: []`,
  `ToolChoice::None`): a candidate that calls a tool is not comparable
  to one that does not, and tool-using loops are what ReAct is for.

### B. `strategy::from_name` — the config-facing half

```rust
pub const KNOWN_STRATEGY_NAMES: &[&str] = &["react", "chain-of-thought", "best-of-n"];

synthia_agent::agent::from_name("CoT")?;   // → ChainOfThoughtStrategy
```
Case-insensitive, with the spellings an operator will actually try
(`re-act`/`re_act`, `chain_of_thought`/`cot`, `best_of_n`/`bon`).
Unknown → `Error::Validation` naming every accepted value, so the
message in a boot log is actionable. Named variants are built with
their defaults; a caller wanting a custom N or scorer assembles the
strategy in code — documented on the function.

### C. The deployment hop

```
agents.<name>.strategy = "chain-of-thought"     (config)
  → AppState::default_strategy  (load_default_strategy, once at boot)
  → RunDependencies::with_optional_strategy
  → AgentRunConfig.strategy
  → AgentRunStreamFactory::run_stream → ReActAgent::with_strategy
  → registered agents (build_react_agent) get the same loop
```

- Resolved **once at boot**: a strategy is immutable and shareable, so
  every run clones one `Arc` instead of re-parsing the config.
- **Fail-soft**: an unknown name is `warn!`ed and treated as unset
  (ReAct) — the same contract `[tools]` and `[mcp_servers]` follow. A
  typo in one agent's table must not stop the server from starting.
- The server has exactly two places that assemble an agent (the session
  factory and `build_react_agent`); both install it, so a session run
  and a registered agent run the same loop.

### D. `completion_to_sampling` is reachable from the provider root

`synthia_provider::completion_to_sampling` (was `traits::…`). A
consumer holding a `CompletionResponse` and publishing an agent event
should not have to know which module the helper lives in; the
re-export is what the new strategy itself uses.

### E. The example covers three paradigms

```
[1/3] ReActStrategy — tools are offered, the loop uses them
      tools offered: 1        tool calls: 1
[2/3] ChainOfThoughtStrategy — no tools, step-by-step prompt
      tools offered: 0        steps parsed: ["read the request.", "answer directly."]
[3/3] BestOfNStrategy — three samples, one winner, no tools
      progress : candidate 1/3 (index 2) scored 0.000 via requires-marker
      progress : candidate 2/3 (index 0) scored 1.000 via requires-marker
      progress : candidate 3/3 (index 1) scored 0.000 via requires-marker
      progress : selected candidate 0 (score 1.000) of 3 in 192.33µs
      samples  : 3            tools offered: 0

STRATEGY-SWAP: OK
```

The two interesting observations are in that output and are asserted,
not just printed: completion order (2, 0, 1) differs from index order —
the samples really are concurrent — and the winner is the sample the
*scorer* preferred, which is the middle-scripted answer, so nothing but
the scorer can be choosing it.

## Verification

| Check | Result |
|---|---|
| `cargo +nightly fmt --all` / `make fmt-check` | clean |
| `cargo clippy --all-targets --all-features --tests --all -- -D warnings` | 0 warnings, 0 errors |
| `cargo test -p synthia-agent` | **325 passed / 0 failed** (317 before: +6 best-of-N, +2 `from_name`) |
| `cargo test -p synthia-server` | **441 passed / 0 failed** (437 before: +2 loader, +1 controller hop, +1 factory end-to-end) |
| Full per-crate sweep (19 crates) | **2612 passed / 0 failed** — core 108, telemetry 36, provider 744, context 95 (103 with `sqlite`), tool 309, session 137, steering 74, skill 56, agent 325, attachment 16, mcp 52, rag 38, scheduler 14, macros 31, eval 36, workflow 65, test-support 18, facade 17, server 441 |
| `cargo run -p synthia-agent --example strategy_swap` | `STRATEGY-SWAP: OK` (three paradigms, output above) |
| `make ci` | green — fmt-check, clippy, MVP dependency set, runtime-free crates, clock ratchet (18 code calls) |
| `make examples` | every example plus both consumer crates (`MVP-OK`) |

### The end-to-end test that matters

`run_factory_runs_the_configured_strategy` builds the **real**
`AgentRunStreamFactory` twice over one registry (one tool `alpha`) and
one recording provider — once with no strategy configured, once with
`from_name("chain-of-thought")` — and asserts the wire tool lists:

```
default (no strategy) → ["alpha"]   (ReAct advertises and would call it)
chain-of-thought      → []          (same registry, nothing offered)
```

That is the whole config path in one assertion: parse → resolve →
thread → install → observable difference on the wire.

Behaviour is unchanged for every existing deployment: with no `strategy`
key the run factory installs nothing and `ReActAgent` keeps
`ReActStrategy`, which is the loop that was already there.

## Deferred to R51 (recorded, not dropped)

- **A judge/verifier scorer shipped in-tree.** `BestOfNStrategy` takes
  any `CandidateScorer`, and `synthia-workflow` already has a
  `GateVerdict`/`SelectionRequest` vocabulary for judging candidates;
  a `synthia-agent`-side LLM-judge scorer (its own `ModelProvider` call,
  scored 0–1) is the obvious next step, and it is a self-contained
  addition now that the seam exists.
- **Per-run strategy override over HTTP.** The knob is per-agent
  (`agents.<name>.strategy`), resolved at boot. A `POST /agents` body
  field or a chat parameter would let one request pick a paradigm —
  worth doing when a client needs it, not speculatively.
- **Wiring `BestOfNStrategy` into the operator-facing agent config with
  a custom N/scorer** (`strategy = { name = "best-of-n", candidates = 5 }`)
  — a config *table* rather than a string. Deferred until someone
  actually needs N ≠ 3 or a scorer other than `LongestAnswer` from a
  config file; the code-level path covers it today.
- Carried: the builtin tools' runtime seam (R40 — declined by design,
  restated in `SEAMS.md`), the `Timer` seam (R39), `cargo-deny` in CI
  (R41), a doctest gate over `SEAMS.md` (R47), a benchmark baseline in
  CI (R48).
