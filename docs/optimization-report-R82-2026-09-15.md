# Optimization report R82 — 2026-09-15

## What the audit found

The round opened with a full re-audit against the
standing objective (absorb the good design from
`traitclaw` / `pi`, keep the workspace modular /
trait-ised / runtime-neutral), not just a grep for the
next drift surface:

- **Reference parity is closed.** Neither
  `~/workspace/traitclaw` nor `~/workspace/pi` has a
  commit since 2026-09-10 — the R62/R64 parity audits
  are current. Every row in
  [`reference-parity.md`](reference-parity.md) maps to
  a verified synthia symbol, is declined with a recorded
  reason, or is a closed gap (the last one, session
  search, closed in R63). There is nothing new to
  absorb.
- **No drift surface is open.** The `MUST match` /
  `MUST stay in sync` grep that drove R76–R80 now hits
  only test assertions (`default_matches_new` and
  friends) — which is exactly where such a claim
  belongs: asserted, not commented.
- **The architecture gates hold.** `make ci` runs
  fmt / clippy / doc, the seven-feature MVP dependency
  invariant, the runtime-free crate invariant, the
  public-API runtime-type ban, and the clock ratchet
  (baseline 6, all intentional) — all green at round
  start.

What remained open was the item R81 named in its
Deferred section: `synthia_tool::registry::tests` was
still the largest flat `mod tests` in the workspace,
and the round started with an **uncommitted, unverified**
`session_scope` extraction sitting in the working tree
from a previous session.

## What landed

Four commits, all the R81 pattern (extract a section
into a sub-module inside the parent `mod tests`, leave
a one-line marker at the original position, touch zero
test logic):

| Commit | Sub-module | Tests moved |
|---|---|---|
| `936a0a7` | `mod session_scope` | the 4 `create_session_scope` lifecycle tests (token allocation, no-op drop, monotonic tokens, registry-dropped-first) — **the in-flight change from the previous session, verified then committed** |
| `9e0af20` | `mod exposure` | the 5 exposure-plumbing tests (`exposure_default_is_direct`, entry→descriptor carry, descriptors-vs-snapshot privacy split, `Registry` get/list preservation, hidden-but-executable dispatch) |
| `2a561bc` | `mod snapshot` | the 7 dual-index snapshot/catalog tests (name-sorted snapshot, unregister reflection, empty registry, entry metadata builders, canonical resolve, materialisation content, scoped-arc drop) |
| `26a5754` | `mod mutations` | the 2 post-registration mutation tests (`set_exposure` projection reach + version-bump semantics; `set_hidden` gating pinned apart from `Hidden` exposure) |

23 of the 283 `synthia-tool` lib tests now live in five
filterable sub-modules (`argument_validation` from R81,
plus the four above). Every filter is verified:

```text
cargo test -p synthia-tool --lib session_scope        → 4 passed
cargo test -p synthia-tool --lib exposure             → 5 passed
cargo test -p synthia-tool --lib snapshot::           → 7 passed
cargo test -p synthia-tool --lib mutations            → 2 passed
cargo test -p synthia-tool --lib argument_validation  → 5 passed (R81)
```

`mod snapshot` repeats `use async_trait::async_trait;`
for the same reason R81 documented for
`mod argument_validation`: the tests derive `Tool` impls
inline and `use super::*;` does not re-export the
macro. `mod exposure` and `mod mutations` need only
`use super::*;` (no inline trait impls).

## Why this is a low-risk refactor

- Every sub-module is `#[cfg(test)]`-only; the public
  surface is unchanged (`make check-public-api-runtime`
  green).
- No test was renamed, no assertion touched, no fixture
  moved. The parent block's shared fixtures
  (`TestEntryTool`, `NamedTool`, `collect_results`)
  stay where they are and reach the sub-modules through
  `use super::*;`.
- The total test count never moved: 283/283 after each
  commit.

## Verification

- `cargo test -p synthia-tool --lib` after each commit:
  283 passed, 0 failed (unchanged total).
- `make ci` 6/6 green (fmt-check, clippy `-D warnings`,
  doc-check, `--no-default-features` compile, MVP
  dependency invariant, runtime-free invariant,
  public-API runtime ban, clock ratchet at baseline 6).
- `make test-unit` all crates green (2447 tests,
  unchanged).
- `make examples` exit 0; `CONSUMER-PROOF: OK` and
  `MVP-OK` proof lines printed.

## Deferred

The remaining flat portion of the parent `mod tests`
block (~1800 lines total block, of which the five
sub-modules now own their concerns) still contains:

- `mod registration` — the Registration/Trait section,
  including the shared fixtures (`TestEntryTool`,
  `ShadowTool`, `NamedTool`, the `Mock*` types). This is
  the largest and most coupled section; it wants a
  `mod fixtures` first so the shared types have one
  home.
- `mod dispatch` — the stream-as-primary-execution-path
  section plus `dispatch_applies_tool_truncate` and the
  `LargeOutputTool` fixture.
- `mod scope` / `mod descriptor_cache` /
  `mod snapshot_cache` — the remaining small sections
  named by R81's deferred list.

Each is a future round on the same pattern. The audit
also confirmed there is no *capability* work waiting:
reference parity is closed and the gates hold, so the
R83+ candidates are organisational (the list above)
unless the reference repos move again.
