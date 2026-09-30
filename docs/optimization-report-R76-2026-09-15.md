# Optimization report R76 — 2026-09-15

## What the audit found

R73 closed the LLM-judge gap and shipped a `parse_judge_score`
helper inside `synthia-agent`. R73's report explicitly
documented the duplication:

> Duplicated rather than re-exported because the eval crate is
> the wrong layer for the agent to depend on — the eval crate is
> for offline grading, the agent runs in production. The two
> stay byte-compatible (same input format, same output domain)
> by design.

The duplication created **drift risk** and **a real bug**: the
two parsers did not accept the same inputs. The agent's parser
was the documented contract (case-insensitive, no-space
variant); the eval's parser was case-sensitive on `Score:` and
rejected `Score:0.5` if the judge elided the space. A judge
that said `score: 0.5` (lowercase) silently scored `0.0` in
`LlmJudgeMetric` and `0.5` in `LlmJudgeScorer`. R73's
"byte-compatible" promise was a lie, and the bug only surfaced
when a deployment wired both.

The right home is **`synthia_core`** — the same crate that
already hosts `validate_against_schema` (the
`StructuredOutputTool` and `SchemaValidationMetric` parser).
Both `synthia-eval` and `synthia-agent` already depend on it,
and the parser is a pure function with no async, no runtime,
no dependencies — exactly the kind of primitive
`synthia-core` exists for.

## What landed

- New module `crates/synthia-core/src/judge_score.rs` —
  `pub fn parse_judge_score(reply: &str) -> f64`. The agent's
  contract (case-insensitive, `Score:0.5` accepted, fallback
  to a standalone number line, clamped to `0.0..=1.0`,
  `0.0` on no match) is now the **only** contract. 7 new
  tests in `synthia-core` (114 total, was 107):
  - `parses_score_prefix_with_space`
  - `parses_score_prefix_without_space`
  - `parses_standalone_number`
  - `finds_score_on_later_line`
  - `clamps_out_of_range`
  - `unknown_reply_scores_zero`
  - `handles_lowercase_and_mixed_case` — the bug R73's
    "byte-compatible" promise hid.
- `pub mod judge_score` in `synthia-core/src/lib.rs`,
  alphabetised; `pub use judge_score::parse_judge_score;` so
  the public surface is `synthia_core::parse_judge_score`.
  The module-doc table also gains the row, alongside the
  other primitives.
- `synthia_eval::metrics::parse_score` — thin wrapper over
  `synthia_core::parse_judge_score`. Same signature, same
  semantics, but now matching the agent's case-insensitive
  contract. The 4 existing eval tests still pass (the new
  cases are a strict superset). No consumer code change
  required.
- `synthia_agent::agent::parse_judge_score` — thin wrapper
  over `synthia_core::parse_judge_score`. The 5 existing
  agent tests still pass (same superset property). The local
  `stripped_prefix_ci` and `parse_first_f64` helpers are
  gone — the 70 lines of duplicated parser are now one
  line of delegation.
- The drift surface is closed: any future change to the
  parser (a new keyword, a different clamp range, a new
  score-line prefix) lands in **one** place and both
  consumers pick it up.

## Verification

- `cargo test -p synthia-core --lib` 114 passed (was 107, +7).
- `cargo test -p synthia-eval --lib` 30 passed (unchanged
  — the 4 existing tests still pass; the new contract
  cases are a superset).
- `cargo test -p synthia-agent --lib` 324 passed (unchanged
  — the 5 existing tests still pass; the duplicated helpers
  are gone, the local `parse_judge_score` is a thin wrapper).
- `make ci` 6/6 green.
- `make test-unit` 2426 passed (was 2419, +7).
- `make examples` exit 0 with `CONSUMER-PROOF: OK` + `MVP-OK`
  + `BEST-OF-N-JUDGE: OK`.
- `make check-mvp-deps` and `make check-public-api-runtime`
  both green: the new module is in `synthia-core` (no HTTP,
  no runtime, no async) and the re-exports are pure `fn`
  re-exports.

## Bug fix

`score: 0.5` (lowercase `s`): the **agent** scored it `0.5`,
the **eval grader** scored it `0.0`. After R76, both score
`0.5`. The bug was unreachable to the existing tests (none
pinned the lowercase case) but the documented contract was
"case-insensitive", and a real judge that varied its casing
would have surfaced it.

## Deferred

None. The shared parser is the one canonical home; both
callers are now thin re-exports. There is no follow-up
maintenance to record.
