# Optimization report R105 — 2026-09-17

## What the audit found

R104 closed the R89 sweep inside `synthia-agent`. The same audit
run workspace-wide found the discipline had never been applied
outside that crate: 66 production files still carried inline test
blocks of 200+ lines, and the five worst offenders held 3.8k lines
of tests interleaved with production code:

| File | Production | Inline tests |
|---|---|---|
| `synthia-workflow/src/runtime.rs` | 1 141 | 1 178 |
| `synthia-provider/src/anthropic/provider/transform.rs` | 426 | 867 |
| `synthia-provider/src/anthropic/provider/mod.rs` | 23 | 861 |
| `synthia-delegation/src/worktree.rs` | 453 | 673 |
| `synthia-context/src/memory/file.rs` | 1 101 | 550 |

`workflow/runtime.rs` was also multi-concern: the
`WorkflowRuntime` facade, the per-run `Executor`, and the
execution-result helpers all lived in one 2 319-line file.

## What landed

**`workflow/runtime.rs` → `runtime/` (one concern per file):**

- `runtime/mod.rs` (268) — module docs, `WorkflowRuntime`
  facade (`run` / `replay_map` / builders), `SKIPPED_MESSAGE`
  (its `runtime::SKIPPED_MESSAGE` path is unchanged —
  `result.rs` imports it), and journal preparation.
- `runtime/executor/mod.rs` (548) — the per-run `Executor`:
  drive, execute, run_step, run_items, run_call, attempt,
  best_of selection, journaling.
- `runtime/executor/mcts.rs` (228) — the MCTS walkthrough
  (`run_mcts`, `run_mcts_row_chained`, `pick_mcts_winner`) as an
  impl continuation, so every production file in the module
  stays under 550 lines.
- `runtime/execution.rs` (165) — how one attempt settles
  (`Attempt`) and how a step's calls become its result
  (`Execution`, `selection`, `first_success`,
  `candidate_outcome`, `rejection_reason`, `chained_prompt`,
  `gate_failure`).
- `runtime/tests.rs` (1 186) — the whole test module, importing
  its names from `crate::` explicitly instead of `super::*`.

**Test-block extraction (pure code motion, prod untouched):**

- `provider/transform/tests.rs` (847; module renamed
  `transform_tests` → `tests` to match the house shape).
- `provider/tests.rs` (856) — `provider/mod.rs` is now the
  26-line module layout it always claimed to be.
- `delegation/worktree/tests.rs` (259; the module's
  `#[allow(dead_code)]` is preserved on the declaration).
- `context/memory/file/tests.rs` (542).

One doc fix rode along: the new `runtime/mod.rs` layout notes
linked `[`Executor`]`, which rustdoc's
`private_intra_doc_links` (part of `make ci` doc-check) rejects
for private items — reworded to plain code formatting in
`runtime/mod.rs` and `runtime/execution.rs`.

## Verification

Test-name lists were captured before any edit and diffed after:

- `synthia-workflow` **64/64**, test set identical
  (`runtime::tests::*` paths unchanged through both the split
  and the MCTS extraction).
- `synthia-provider` **690/690**, test set identical modulo the
  intentional `transform_tests` → `tests` module rename.
- `synthia-delegation` **66/66** and `synthia-context`
  **95/95**, byte-identical test lists.
- `make test-sqlite` **103/103** (context with `sqlite`).
- `make ci` **7/7 green** — including `doc-check`, which caught
  the private-link regression above before it could land.
- Workspace `cargo clippy --all-targets --all-features --tests
  --all`: **0 warnings**; `cargo +nightly fmt --all --check`
  clean.

## Result

Exactly one production file workspace-wide still holds an
inline test block ≥ 500 lines (`session/compaction_checkpoint.rs`
at 501); every other block that size or larger is gone. The next
rounds of the sweep, in descending value:

1. `compaction_checkpoint.rs` (501-line block) and the remaining
   200–500-line blocks (`session/events.rs` 480,
   `tool/surface.rs` 496, `core/error.rs` 452,
   `provider/retry.rs` 449, …).
2. The remaining multi-concern production files outside
   `synthia-agent`: `server/session/controller.rs` (2 328),
   `tool/registry.rs` (1 365), `context/memory/file.rs`
   (1 101 production), `server/state/app_state.rs` (1 071).
