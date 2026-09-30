# Optimization report R117 — 2026-09-18

## What the audit found

The R109 test-layout ratchet still carried 18 production files
whose single inline `#[cfg(test)] mod` block was ≥300 lines —
the band the rule says may only shrink. Ten lived outside
`synthia-provider`/`synthia-server`; eight inside them.

## What landed

Every one moved to a sibling `tests.rs` (the rule's prescribed
home), via `#[cfg(test)] mod tests;`:

| crate | file | before → after | sibling |
|---|---|---|---|
| session | `surface.rs` | 632 → 261 | `surface/tests.rs` |
| session | `operation.rs` | 630 → 464 | `operation/tests.rs` |
| context | `context_manager.rs` | 860 → 532 | `context_manager/tests.rs` |
| tool | `types.rs` | 563 → 241 | `types/tests.rs` |
| tool | `truncate/bound_output.rs` | 742 → 532 | `truncate/bound_output/tests.rs` |
| steering | `hook.rs` | 757 → 452 | `hook/tests.rs` |
| steering | `hook_map.rs` | 668 → 361 | `hook_map/tests.rs` |
| tool-shell | `lib.rs` | 811 → 486 | `tests.rs` |
| tool-write | `lib.rs` | 539 → 183 | `tests.rs` |
| test-support | `replay.rs` | 957 → 593 | `replay/tests.rs` |
| provider | `config.rs` | 702 → 320 | `config/tests.rs` |
| provider | `assembler.rs` | 687 → 382 | `assembler/tests.rs` |
| provider | `cache_policy.rs` | 561 → 238 | `cache_policy/tests.rs` |
| provider | `openai_streaming/processor.rs` | 711 → 326 | `processor/tests.rs` |
| provider | `anthropic/provider/parse.rs` | 458 → 135 | `parse/tests.rs` |
| provider | `types/stream_chunk.rs` | 466 → 120 | `stream_chunk/tests.rs` |
| provider | `types/completion.rs` | 420 → 99 | `completion/tests.rs` |
| server | `config/server.rs` | 732 → 384 | `config/server/tests.rs` |

Two files carried a *second*, smaller inline block
(`operation.rs`'s `bus_tests`, `bound_output.rs`'s
`default_tests`) — those stay inline, per the R111 precedent
that only the ≥300 band is tracked.

Two mechanical details the sweep had to get right:

- `mod tests;` must sit at EOF when the block was the last item;
  in `operation.rs` / `bound_output.rs` the declaration was
  hoisted to the file end so the sibling resolves.
- `provider/config.rs`'s block embeds a column-0 raw TOML
  fixture; the de-indent rule (strip exactly 4 spaces) keeps the
  literal intact.

**One test replaced, not re-pinned.** `server/config/server.rs`'s
`the_cli_declares_the_env_vars_the_images_set` pinned the
`--host` / `--port` env wiring by asserting on this repo's
*source text* (`include_str!("../main.rs")` +
`source.contains("env = \"SYNTHIA_HOST\"")`) — a source-text pin
rather than a contract check, and one whose relative path broke
the moment the block moved a directory deeper. It is **not**
re-pointed at `../../main.rs`; the contract it claimed is
re-asserted properly in R119 by
`crates/synthia-server/src/main.rs`'s
`the_cli_binds_the_env_vars_the_images_set`, which reads the
parsed CLI model (`Args::command()` → `get_env()`) so a renamed
env var or a dropped `env = …` attribute fails. Mutation-checked:
flipping the attribute to `SYNTHIA_HOST_TYPO` fails that test.
Note the `BindAddress::resolve` tests cover host/port
*precedence*, a different contract. Server lib tests 403 → 402;
the bin target gains 1.

**Ratchet closed.** `TEST_BLOCK_BASELINE` 18 → **0**; AGENTS.md
§3.6 updated. The band is now empty: no production file carries
an inline test module ≥300 lines, and the gate fails on the
first new one.

## Verification

- Per-crate lib suites: session 147, context 95, tool 215,
  steering 72, tool-shell 17, tool-write 14, test-support 19,
  provider 690, server 402 — all pass.
- Test-name parity spot-checked on three crates
  (`surface`/`hook`/`tool-write`): identical counts in the
  sibling files.
- `cargo clippy --all-targets --all-features --tests --all
  -- -D warnings` — 0 warnings.
- `cargo +nightly fmt --all`; `make ci` **8/8** with
  `0 inline test module(s) >=300 lines (baseline 0)`.

## Result

The test-layout rule is now fully satisfied rather than
ratcheted: every production file ends at `mod tests;` (or
carries only a small block), and the six-line-`use`-block plus
deep test bodies live next to the module they exercise.
