# Optimization report R112 — 2026-09-18

## What the audit found

`synthia-delegation` — the plugin crate the objective names
explicitly ("将 subagent 的 task tool 也拆分出来") — was the
last crate in the R104–R111 "one concern per file" series that
had not been through the split. Three files carried everything:

- `pool.rs` — 1036 lines: slot taxonomy, admission tickets,
  invocation records, and the two-lane state machine in one
  file, plus a 348-line inline test block (a member of the
  ≥300 band the R109 ratchet tracks).
- `task.rs` — 874 lines: spec parsing, wire schema, the
  interceptor plugin, and the child-run driver.
- `gate.rs` — 840 lines: single concern already, but carrying a
  330-line inline test block (another band member).

`task.rs` also hid ~90 lines of dead weight:
`task_tool_definition_with_features` built a complete
`ToolSchemaBuilder` schema, discarded it (`let _ = schema;`),
and rebuilt the canonical one through `build_final_schema` —
plus a `required.retain` that could never remove anything
(`isolation` was never in `required`).

## What landed

**`pool/` — five files, one concern each.**

| file | lines | concern |
|---|---|---|
| `slot.rs` | 62 | `Slot` lanes + `DEFAULT_BACKGROUND_CONCURRENCY` |
| `admission.rs` | 86 | `AdmissionTicket` / `Admission` / `QueuedToken` |
| `record.rs` | 163 | `InvocationStatus` / `InvocationRecord` / `MAX_TOMBSTONES` |
| `state.rs` | 329 | `SubagentPool` + `PoolCounts` + the pool-id source |
| `mod.rs` | 80 | module docs + `pub use` wiring (paths unchanged) |
| `tests.rs` | 341 | the moved 348-line inline block |

Struct fields on the admission types are `pub(super)` — only
the state machine can mint a ticket; `Slot::index` likewise.
Nothing new is `pub`: every pre-existing path
(`synthia_delegation::pool::SubagentPool`, the crate-root
re-exports, the facade) resolves exactly as before.

**`task/` — five files, one concern each.**

| file | lines | concern |
|---|---|---|
| `spec.rs` | 124 | `TaskSpec` + gate/isolation field parsing + the two consts |
| `schema.rs` | 107 | the LLM-facing wire definition (dead first pass deleted) |
| `delegator.rs` | 128 | `TaskDelegator` + `ToolInterceptor` impl |
| `runner.rs` | 244 | child-run driver + gate/worktree lifecycle |
| `mod.rs` | 74 | module docs + `pub use` wiring |
| `tests.rs` | 177 | the moved inline block |

The simplified `task_tool_definition_with_features` is
behaviour-identical: the three wire-shape tests
(`has_required_schema_shape`, `feature_on_advertises_isolation`,
`feature_off_drops_isolation`) pass unmodified.

**`gate.rs` 840 → 513 + `gate/tests.rs` 314.** The module keeps
its `#[cfg(test)] #[allow(...)] mod tests;` declaration — the
allowances travel with the declaration, the content moved.

**Band ratchet:** the ≥300-line inline-test tier drops
**20 → 18**; `TEST_BLOCK_BASELINE` lowered and AGENTS.md §3.6
updated in the same change.

## Verification

- `synthia-delegation`: **72/72** — 66 lib (test-name set
  byte-identical before/after the split, verified by diffing
  `--list` output against `HEAD`), 4 doc, 2 integration.
- `synthia-server` **403/403**, `synthia-agent` **240/240** —
  unchanged from R111.
- Examples print their proof lines: `DELEGATION-GATE: OK`,
  `WORKTREE-ISOLATION: OK`.
- `cargo clippy --all-targets --all-features --tests --all
  -- -D warnings` — 0 warnings.
- `cargo +nightly fmt --all` applied; `make ci` **8/8** OK with
  the new baseline (`18 inline test module(s) >=300 lines`).
- `RUSTDOCFLAGS="-D warnings" cargo doc` clean (two doc-link
  fixes were needed after the move: lifecycle references in
  `admission.rs` de-linked, external links in `task/mod.rs`
  fully qualified).

## Result

The delegation crate now reads like the agent crate does after
R107–R111: open `pool/` or `task/`, pick the file named after
the question. The crate's public surface is untouched — zero
callers outside the crate changed — and the two band members it
contributed to the test-layout ratchet are gone.

Net: 2742 lines across 14 focused files (was 2750 across 3),
~90 lines of dead schema code deleted, largest production file
in the crate now 513 lines (was 1036).
