# Optimization report R100 — 2026-09-17

## What the audit found

R99 split the strategy seam. The follow-up audit turned to the
largest remaining multi-concern file in `synthia-agent`:
`agent/builder.rs` (991 lines) held **three** concerns side by
side:

| Lines | Concern |
|---|---|
| 1–420 | `AgentBuilder` — the struct, every setter, `build()` |
| 422–604 | The compaction wiring: `CompactionEmitters`, the `record_view` / `lifecycle_view` mirrors, `context_manager_for_compaction(_with_emitters)`, and the R13-3 `resolve_context_manager` decision core |
| 605–991 | tests (385 lines) |

The compaction half is not builder-private machinery: the
server's run factory (`synthia-server/src/session/controller.rs`)
calls `context_manager_for_compaction_with_emitters` directly when
it hand-assembles agents, and `CompactionEmitters` is re-exported
at the crate root for the same reason. Two different consumers,
one file — the reader of either had to skip past the other's code.

## What landed

Pure code motion, public paths unchanged. `builder.rs` became a
`builder/` directory:

```
builder/mod.rs          432 lines   AgentBuilder struct + every setter + build()
builder/compaction.rs   212 lines   CompactionEmitters + view mirrors + the public
                                    context_manager_for_compaction* entries +
                                    resolve_context_manager (pub(super))
builder/tests.rs        378 lines   builder contract tests + R34 durable-checkpoint tests
```

`mod.rs` re-exports the compaction public API
(`pub use compaction::{CompactionEmitters, context_manager_for_compaction,
context_manager_for_compaction_with_emitters}`), so every existing
path — `synthia_agent::agent::builder::*`, the `agent/mod.rs`
re-exports, and the crate-root `synthia_agent::{CompactionEmitters,
context_manager_for_compaction*}` — resolves unchanged.
`resolve_context_manager` widened from file-private to
`pub(super)` (visible inside the `builder` subtree only) so the
sibling test module keeps driving the decision table directly.

### Why the split is safe for the server

`synthia-server`'s controller imports via the crate root
(`synthia_agent::CompactionEmitters`,
`synthia_agent::context_manager_for_compaction_with_emitters`) —
none of its call sites name the module path, and the re-export
chain is compiler-checked. 403/403 server lib tests pass
untouched.

## Verification

- `make ci` **7/7 green** (fmt-check, clippy `-D warnings`,
  rustdoc `-D warnings` — two moved doc links were rewritten to
  `super::AgentBuilder` / `synthia_session::CompactionCheckpoint`
  targets — MVP-deps, runtime-free, public-API-runtime,
  claim-language, clock).
- `cargo test -p synthia-agent --lib` **247/247** — baseline
  identical; the 10 builder/compaction tests run at
  `agent::builder::tests::*` unchanged.
- `cargo test -p synthia-server --lib` **403/403** (the direct
  consumer of the moved API).
- `cargo test -p synthia-delegation --lib` **66/66**.
- Examples: `assemble_from_zero` → `ASSEMBLE-FROM-ZERO: OK`;
  `runtime_agnostic` → `RUNTIME-AGNOSTIC: OK`.
- `cargo +nightly fmt --all --check` clean.

## Result

`builder/` now has the same one-concern-per-file property as the
loop (`loop_/`), the strategy seam (`strategy/`), and the facade.
The biggest remaining file in `synthia-agent/src/agent/` above
500 lines is `team.rs` (900), which is three *independent*
compositions sharing one file — a different treatment (per-type
modules under `team/`) flagged for a future round, same as
`best_of_n.rs` (676, single strategy + its tests).
