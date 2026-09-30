# Optimization report R73 — 2026-09-15

## What the audit found

R72's `make ci` was green; the 11 known gaps in `docs/README.md` were
re-audited. Most require a maintainer call (regenerate replace semantics,
projection JSON clones, `cargo-deny` DB fetch, SEAMS.md doctest gate,
the runtime seam for the builtin tools, the `Timer` seam,
`GroupedRegistry` naming, the frontend regression suite, the
regenerated-session view divergence, the chat-parameter strategy
override). One gap was both **autonomously feasible** and on the R50
priority list:

> **An in-tree LLM-judge scorer** | `BestOfNStrategy` takes any
> `CandidateScorer`; a judge that calls a provider itself is the obvious
> next one. R50.

The audit's two structural findings:

1. The trait `CandidateScorer::score` was `fn score(&self, candidate: &str) -> f64`
   — **sync**. An LLM judge must make a provider call. The eval crate
   already had `LlmJudgeMetric<P: JudgeProvider>` (a `JudgeProvider` seam
   with its own `parse_score`), but the eval crate is for offline
   grading — coupling `synthia-agent` → `synthia-eval` would put a runtime
   dep on a crate that is for tests, which is the wrong direction.
2. The three in-tree `impl CandidateScorer` are `LongestAnswer` (default),
   the README's `PrefersShorter`, and the test-only `Shortest` —
   all sync. Making the trait async is additive (every existing impl
   gains `async fn` and a `#[async_trait]` annotation; the body is
   unchanged).

## What landed

- `CandidateScorer::score` is now `async fn score(&self, candidate: &str) -> f64`.
  The trait gained `#[async_trait]`; the doc comment notes that pure
  scorers stay synchronously cheap and an LLM judge simply awaits its
  provider call. `BestOfNStrategy::run` awaits the scorer on the candidate's
  own spawned task — no new `tokio::spawn` (the runtime's spawner already
  carries the candidate work).
- `LlmJudgeScorer` in `crates/synthia-agent/src/agent/best_of_n_llm_judge.rs`:
  - Holds `Arc<dyn ModelProvider>` (any provider the agent can call, so
    the judge and the candidates can share a model or use a different one
    per deployment).
  - `LlmJudgeScorer::new(provider)` uses the in-tree rubric
    `DEFAULT_JUDGE_PROMPT`; `with_rubric(provider, rubric)` is the seam
    for a deployment's own criteria.
  - `with_max_tokens(u32)` caps the judge's reply; the default is 32,
    enough for `Score: 0.85` plus margin.
  - `parse_judge_score(reply: &str) -> f64` is the same shape
    `synthia_eval::metrics::parse_score` uses (`Score: …` on any line,
    case-insensitive, clamped to `0.0..=1.0`, fallback to a standalone
    number, `0.0` on no match). Duplicated rather than re-exported
    because the eval crate is the wrong layer for the agent to depend
    on — the eval crate is for offline grading, the agent runs in
    production. The two stay byte-compatible (same input format, same
    output domain) by design.
  - Provider error path: returns `0.5` (neutral) and logs. Returning `0.0`
    would tie every failed judge at the bottom; returning `1.0` would
    bury the strategy's own failure path (a `provider.complete` error on
    a judge should not be conflated with a candidate that the judge
    itself scored 1.0). `0.5` keeps the judge out of the ranking while
    the strategy's own error path records the failure.
- `pub use best_of_n_llm_judge::{LlmJudgeScorer, DEFAULT_JUDGE_PROMPT};`
  in `crates/synthia-agent/src/agent/mod.rs`.
- README example updated to `async fn score`; the seam is documented
  as "A real signal — a verifier, a test run, an LLM judge — is also a
  `CandidateScorer`".
- The `strategy_swap.rs` example's `RequiresMarker` scorer gained
  `#[async_trait]`. The body is unchanged.

## Verification

- `cargo test -p synthia-agent --lib` 324/324 (was 316, +8 for the judge).
  - 5 tests cover `parse_judge_score` directly
    (`parses_score_prefix`, `parses_standalone_number`,
    `clamps_out_of_range`, `unknown_reply_scores_zero`,
    `finds_score_on_later_line`).
  - 3 cover the scorer through a stub provider
    (`judge_scorer_returns_parsed_value`,
    `judge_scorer_falls_back_to_neutral_on_provider_error`,
    `judge_prompt_carries_rubric_and_candidate`).
- `cargo test -p synthia-agent --lib best_of_n` 6/6 — the existing
  `the_scorer_decides_which_candidate_wins` and `ties_go_to_the_lowest_index`
  tests still pin the strategy's behaviour with a sync `Shortest` scorer.
- `cargo build -p synthia-agent --examples` — `strategy_swap.rs`
  compiles, prints `STRATEGY-SWAP: OK`.
- `make ci` 6/6 green (`fmt-check`, `lint-rust`, `doc-check`,
  `check-mvp-deps`, `check-no-runtime`, `check-public-api-runtime`,
  `check-clock` — 6 chrono `Utc::now()` calls in library code, baseline 6).
- `make test-unit` 2414/2414 pass across the per-crate runs.
- `make examples` exit 0 with `CONSUMER-PROOF: OK` + `MVP-OK`.

## What also landed after the initial commit

- `crates/synthia-agent/examples/best_of_n_judge.rs` — a runnable
  demo of `BestOfNStrategy` with `LlmJudgeScorer`. A scripted
  provider splits its replies between three candidates and three
  judge scores (deliberately *mismatched* with candidate order so
  the example proves the judge — not streaming order — picks the
  winner). The example prints every progress event, including the
  per-candidate judge score, and asserts:
  - 3 candidate calls and 3 judge calls (one each per candidate);
  - the highest-scoring candidate wins.

  Output ends with `BEST-OF-N-JUDGE: OK`. Now exercised by
  `make examples`, which lists it alongside the other proof lines
  (`MVP-OK`, `CONSUMER-PROOF: OK`).

## Deferred to the next round

None from this round — the trait change is additive and the new
scorer is a peer of `LongestAnswer`. The remaining 10 gaps
(`docs/README.md`) all need a maintainer decision or a new commit
under a different rubric; recorded in the round-map table.
