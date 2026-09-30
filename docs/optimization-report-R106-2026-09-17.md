# Optimization report R106 — 2026-09-17

## What the audit found

R105 resolved the five worst inline test blocks (≥550 lines)
but left the next tier untouched: twelve production files in six
crates each interleaved 400–500 lines of tests with production
code — `session/compaction_checkpoint.rs` (501),
`session/events.rs` (480), `tool/surface.rs` (496),
`core/error.rs` (452), `provider/retry.rs` (449),
`session/repair.rs` (444), `provider/streaming/anthropic/processor.rs`
(430), `session/token_meter.rs` (427),
`context/summarizing.rs` (419), `core/registry.rs` (416),
`tool/sandbox.rs` (414), `workflow/plan.rs` (409).

## What landed

Pure code motion, one extraction per file, prod code untouched:

| Production file (after) | Tests moved to |
|---|---|
| `session/compaction_checkpoint.rs` 281 | `compaction_checkpoint/tests.rs` 498 |
| `session/events.rs` 831 | `events/tests.rs` 477 |
| `session/repair.rs` 376 | `repair/tests.rs` 441 |
| `session/token_meter.rs` 550 | `token_meter/tests.rs` 424 |
| `tool/surface.rs` 455 | `surface/tests.rs` 493 |
| `tool/sandbox.rs` 663 | `sandbox/tests.rs` 411 |
| `core/error.rs` 442 | `error/tests.rs` 449 |
| `core/registry.rs` 313 | `registry/tests.rs` 413 |
| `provider/retry.rs` 549 | `retry/tests.rs` 446 |
| `provider/streaming/anthropic/processor.rs` 347 | `streaming/anthropic/processor/tests.rs` 427 |
| `context/summarizing.rs` 768 | `summarizing/tests.rs` 416 |
| `workflow/plan.rs` 683 | `plan/tests.rs` 406 |

Every file already used the single `mod tests` at EOF shape, so
each extraction is: slice the module body, dedent one level,
write it to `<module>/tests.rs`, replace the block with
`#[cfg(test)] mod tests;`. The only physical string literals
found in the bodies were `\`-continuations (leading whitespace
stripped by the compiler) — dedent-safe by construction.

## Verification

Test-name lists captured before the batch and diffed after, per
crate:

- `synthia-core` **151/151**, `synthia-tool` **215/215**,
  `synthia-session` **147/147**, `synthia-provider` **690/690**,
  `synthia-context` **95/95**, `synthia-workflow` **64/64** —
  all six lists byte-identical.
- Downstream: `synthia-agent` **247/247**,
  `synthia-delegation` **66/66**, `synthia-server` **403/403**.
- `make ci` **7/7** (exit 0); workspace clippy
  `--all-targets --all-features --tests --all` **0 warnings**;
  `cargo +nightly fmt --all --check` clean.

## Result

Workspace-wide, inline test blocks in production files are now
uniformly **< 400 lines** — exactly two remain at 400–401
(`provider/types/content.rs`, `provider/openai/types.rs`), and
22 sit in the 300–400 band. The sweep continues to pay down that
band the same way; after it, the remaining layout work is
multi-concern production code: `server/session/controller.rs`
(2 328), `tool/registry.rs` (1 365), `context/memory/file.rs`
(1 101), `server/state/app_state.rs` (1 071).
