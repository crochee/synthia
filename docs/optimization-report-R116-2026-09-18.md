# Optimization report R116 — 2026-09-18

## What the audit found

`synthia-context/src/dag.rs` (941 lines) held the hierarchical
DAG compaction tier: config knobs, the in-memory DAG store and
its nodes, the two-pass compaction engine + `ContextManager`
impl, the three summariser prompts (verbatim `pi-lcm` ports),
and the serialisation/chunking helpers, with a 243-line inline
test block.

## What landed

`dag.rs` → `dag/`:

| file | lines | concern |
|---|---|---|
| `mod.rs` | 77 | module docs + `COMPACTION_TAG` / `SummaryId` + re-exports |
| `config.rs` | 54 | `DagConfig` + `pi-lcm` defaults + `validated` clamps |
| `store.rs` | 116 | `DagNode` + `DagStore` (lossless raw-text map) |
| `manager.rs` | 372 | `DagContextManager`: leaf/condensed passes, `assemble_view`, `describe`/`expand`, `ContextManager` impl |
| `prompts.rs` | 50 | the three prompts (including their injection guards) |
| `serialise.rs` | 92 | message serialisation, token estimation, chunking |
| `tests.rs` | 247 | the moved inline block |

`DagStore`'s mutation methods (`insert`, `mark_consumed`,
`unconsumed_at_depth`, `mint_message_ids`, `store_raw_texts`) are
`pub(super)`; the text/prompt helpers likewise. Nothing new is
`pub` — `synthia_context::dag::{DagConfig, DagContextManager,
DagNode, DagStore, SummaryId}` and the crate root are unchanged.

## Verification

- `synthia-context` **95/95** lib + **103/103** `--features
  sqlite`.
- `synthia-server` **403/403** unchanged.
- `cargo clippy --all-targets --all-features --tests --all
  -- -D warnings` — 0 warnings.
- `cargo +nightly fmt --all`; `make ci` **8/8**.

## Result

The compaction engine reads as documented: the `pi-lcm` mapping
table in the module docs now names one file per row
(`config` / `store` / `manager` / `prompts`). Largest file
941 → 372, and the verbatim-ported prompts are isolated so a
future prompt edit cannot touch engine logic.
