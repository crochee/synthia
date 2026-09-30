# Optimization report R84 — 2026-09-15

## What the audit found

R83 named `mod registration` as the immediate
follow-up: "the Registration/Trait block (~700 lines,
including descriptor/snapshot cache tests) is ready for
extraction on the same pattern as R81/R82's
sub-modules" — but only after `mod fixtures` made it
a pure section move.

R83 landed that prerequisite. This round executes the
named follow-up.

- Reference parity remains closed (no `traitclaw` /
  `pi` commit since 2026-09-10).
- No drift surface appeared (`MUST match` / `MUST stay
  in sync` grep hits only test assertions and
  historical "closed by const alias" notes).
- `make ci` was green at round start; the audit's only
  candidate was the named follow-up.

## What landed

One commit:

- `mod registration` at the parent `mod tests` block
  (lines 1744–2375 pre-extraction, ~630 lines,
  30 tests). The 30 tests cover:
  - `register_entry` accept / refuse / increment
    (`register_entry_cannot_shadow_core_tool`,
    `register_entry_for_new_name_returns_true_and_inserts`)
  - `unregister_by_name` returns the right `bool`
  - descriptor / snapshot caches invalidate on change
    and survive a clone
  - snapshot is alphabetically sorted, collapses
    duplicates, filters hidden tools
  - `get` / `list` / `contains_and_len` through the
    public surface and the `Registry` trait
  - dispatch-shape coverage with one-off `Tool` impls
    (`ToolWithRequired` for schema-validation error,
    `FastTool` for concurrent dispatch,
    `HiddenTool` shapes for visibility gating).
- R83's `mod fixtures` made this a pure section move:
  every shared `Tool` impl now lives in one focused
  sub-module, so this extraction drags no shared
  state with it. The few tests that derive their own
  one-off `Tool` impls for dispatch-shape coverage
  move into the sub-module too (they are local to
  those tests).
- `super::super::registry_trait::ToolFilter` for the
  two `Registry`-trait filter tests (private sibling
  module).
- No test renamed, no assertion touched, no fixture
  behaviour changes.

## Why this is a low-risk refactor

- Every existing call site compiles via R83's
  `pub(crate) use fixtures::{...};` re-export.
- `mod registration` is `#[cfg(test)]`-only; no public
  surface moves (`make check-public-api-runtime`
  green).
- 30 of the 283 `synthia-tool` lib tests now live in a
  filterable sub-module, accessible via
  `cargo test -p synthia-tool --lib registration`
  (24 of those filter on the name "registration";
  the remainder use shadow/fast/required/hidden that
  don't match the substring). The parent block gains a
  one-line marker at the original position.

## Verification

- `cargo test -p synthia-tool --lib registration`:
  24 passed, 259 filtered out.
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

- `mod dispatch` — the stream-as-primary-execution-path
  section plus `dispatch_applies_tool_truncate` and the
  `LargeOutputTool` fixture (~250 lines).
- `mod scope` / `mod descriptor_cache` /
  `mod snapshot_cache` — the remaining small sections
  named by R81's deferred list.

Six sub-modules now own focused concerns within the
parent `mod tests` block: `argument_validation` (R81),
`session_scope` (R82), `exposure` (R82), `snapshot`
(R82), `mutations` (R82), `fixtures` (R83), and
`registration` (R84). 23 of 30 registration tests plus
all the R82 sub-modules' tests — over half of the
283 lib tests — are now filterable by name.

R85+ candidates are organisational until the reference
repos move again.