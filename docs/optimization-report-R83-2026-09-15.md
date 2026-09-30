# Optimization report R83 — 2026-09-15

## What the audit found

The round opened by re-verifying the R82 audit (parity
closed, no drift, gates green) and looked for the next
self-contained move. R82's Deferred section named `mod
fixtures` as the prerequisite for `mod registration`:

> the Registration/Trait section, including the shared
> fixtures (`TestEntryTool`, `ShadowTool`, `NamedTool`).
> This is the largest and most coupled section; it wants
> a `mod fixtures` first so the shared types have one
> home.

Three `Tool` impls were defined inline in the
Registration/Trait section but referenced from every
section of the parent `mod tests` block
(registration, exposure, session_scope, snapshot,
descriptor cache, mutations) — six sections, three
fixtures, eighteen cross-references. Each fixture had
one canonical definition, but the *location* was wrong:
the registration section was the accidental home.

## What landed

One commit:

- `pub mod fixtures` at the top of the parent `mod tests`
  block. `TestEntryTool`, `ShadowTool`, `NamedTool` move
  there once. `use async_trait::async_trait;` is
  repeated for the same reason `mod snapshot` /
  `mod argument_validation` repeat it (`use super::*;`
  does not re-export the macro).
- `pub(crate) use fixtures::{NamedTool, ShadowTool,
  TestEntryTool};` at the parent level re-exports the
  three types so every existing test body — every
  `Arc::new(TestEntryTool)`, every `NamedTool("alpha")`,
  every `Arc::new(ShadowTool)` — keeps compiling
  unchanged.
- No test body, no assertion, no fixture behaviour
  changes. The fixture impls are byte-for-byte identical
  to what they were.

## Why this is a low-risk refactor

- Every existing call site compiles via the
  `pub(crate) use` re-export. The diff is purely
  organisational: 114 lines added, 81 removed.
- `mod fixtures` is `#[cfg(test)]`-only and the re-export
  inherits the parent's test scope; nothing in the
  public surface moves (`make check-public-api-runtime`
  green).
- The shared fixtures now have a single home, so the
  next extraction (`mod registration`, R84) becomes
  a pure section move without dragging fixture types
  with it.

## Verification

- `cargo test -p synthia-tool --lib` after the commit:
  283 passed, 0 failed (unchanged total).
- `make ci` 6/6 green (fmt-check, clippy `-D warnings`,
  doc-check, `--no-default-features` compile, MVP
  dependency invariant, runtime-free invariant,
  public-API runtime ban, clock ratchet at baseline 6).
- `make examples` exit 0; `CONSUMER-PROOF: OK` and
  `MVP-OK` proof lines printed.

## Deferred

- `mod registration` — the now-fixture-free
  Registration/Trait block (~700 lines, including
  descriptor/snapshot cache tests) is ready for
  extraction on the same pattern as R81/R82's
  sub-modules. R84 candidate.
- `mod dispatch` — the stream-as-primary-execution-path
  section plus `dispatch_applies_tool_truncate` and the
  `LargeOutputTool` fixture (~250 lines).
- `mod scope` / `mod descriptor_cache` /
  `mod snapshot_cache` — the remaining small sections
  named by R81's deferred list.

Reference parity remains closed; no `traitclaw` / `pi`
commit since 2026-09-10. The audit's R84+ candidates
are organisational until the reference repos move
again.