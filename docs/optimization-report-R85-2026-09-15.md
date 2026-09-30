# Optimization report R85 — 2026-09-15

## What the audit found

R84 named `mod dispatch` as the next R85 candidate:
"the stream-as-primary-execution-path section plus
`dispatch_applies_tool_truncate` and the
`LargeOutputTool` fixture (~250 lines)". This round
executes that named follow-up.

- Reference parity remains closed (no `traitclaw` /
  `pi` commit since 2026-09-10).
- No drift surface appeared (`MUST match` / `MUST stay
  in sync` grep hits only test assertions and
  historical "closed by const alias" notes).
- `make ci` was green at round start; the audit's only
  candidate was the named follow-up.

## What landed

One commit:

- `mod dispatch` at the parent `mod tests` block
  (lines 2611–2905 pre-extraction, ~290 lines,
  6 tests + 4 local fixtures). The 6 tests cover:
  - per-call truncation bound applied to a too-large
    output (`dispatch_applies_tool_truncate`)
  - streaming dispatch collects the final `Result` while
    dropping progress items
    (`dispatch_consumes_stream_collects_final_result`)
  - a stream that yields no `Result` is a contract
    violation (two shapes: empty stream via
    `dispatch_returns_error_when_stream_yields_no_result`,
    progress-only stream via
    `dispatch_treats_no_result_as_contract_violation`)
  - `snapshot_with_provenance` carries the
    `ToolProvenance::Dynamic` flag and filters + sorts
    (`snapshot_with_provenance_returns_records_with_provenance`,
    `snapshot_with_provenance_skips_hidden_and_sorts`).
- The four local `Tool` fixtures (`LargeOutputTool`,
  `StreamingTool`, `EmptyStreamTool`, `ProgressOnlyTool`)
  travel with the section — each is referenced only
  by these tests.
- `async_trait` + `futures::stream` are repeated inside
  the sub-module because `use super::*;` does not
  re-export them.
- No test renamed, no assertion touched, no fixture
  behaviour changes.

## Why this is a low-risk refactor

- Every existing call site compiles via `use super::*;`
  and the repeated imports for `async_trait` and
  `futures::stream`.
- `mod dispatch` is `#[cfg(test)]`-only; no public
  surface moves (`make check-public-api-runtime`
  green).
- The 4 local fixtures (`LargeOutputTool`,
  `StreamingTool`, `EmptyStreamTool`, `ProgressOnlyTool`)
  now live alongside their tests in the sub-module;
  the parent's shared fixtures stay in `mod fixtures`
  (R83) and remain reachable via the
  `pub(crate) use` re-export.

## Verification

- `cargo test -p synthia-tool --lib dispatch`:
  17 passed, 266 filtered out (the 4 fixtures are
  `#[derive(Debug)]` structs without tests, so they
  don't filter; the 6 new `dispatch::*` tests plus the
  11 existing `dispatch_*` tests in `mod registration`
  both match the `dispatch` substring).
- `cargo test -p synthia-tool --lib` after the commit:
  283 passed, 0 failed (unchanged total).
- `make ci` 6/6 green (fmt-check, clippy `-D warnings`,
  doc-check, `--no-default-features` compile, MVP
  dependency invariant, runtime-free invariant,
  public-API runtime ban, clock ratchet at baseline 6).
- `make examples` exit 0; `CONSUMER-PROOF: OK` and
  `MVP-OK` proof lines printed.

## Deferred

The remaining flat portion of the parent `mod tests`
block is now:

- `mod scope` / `mod descriptor_cache` /
  `mod snapshot_cache` — the remaining small sections
  named by R81's deferred list.

Eight sub-modules now own focused concerns within the
parent `mod tests` block: `argument_validation` (R81),
`session_scope` (R82), `exposure` (R82), `snapshot`
(R82), `mutations` (R82), `fixtures` (R83),
`registration` (R84), and `dispatch` (R85). The parent
block is now small enough that the remaining three
sections can be one or two R86+ rounds on the same
pattern.

R86+ candidates are organisational until the reference
repos move again.