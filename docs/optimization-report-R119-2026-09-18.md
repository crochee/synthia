# Optimization report R119 — 2026-09-18

## What the audit found

`synthia-context/src/summarizing.rs` (770 lines) held six
concerns in one file: the `ToolCallRecord` / `ToolCallArchive`
pair, the two emit types (`CompactionRecord`, and the
three-step `CompactionLifecycle`), the summariser prompt
serialisation, the short-alias + summary-message rendering, and
the `SummarizingContextManager` itself with its `ContextManager`
impl — plus a 410-line sibling test file.

## What landed

`summarizing.rs` → `summarizing/`:

| file | lines | concern |
|---|---|---|
| `mod.rs` | 83 | module docs + layout + `SUMMARY_TAG` / `SummariseFn` + re-exports |
| `archive.rs` | 116 | `ToolCallRecord` + `ToolCallArchive` + the record builder |
| `lifecycle.rs` | 75 | `CompactionRecord` + `CompactionLifecycle` |
| `prompt.rs` | 120 | batch serialisation + the R29-C details line |
| `refs.rs` | 38 | short aliases (`t1`, `t2`, …) + the summary message |
| `manager.rs` | 405 | `SummarizingContextManager` + the `ContextManager` impl |
| `tests.rs` | 429 | the existing sibling suite (imports made explicit) |

Cross-module helpers (`archive_record_for`,
`extract_tool_result_text`, `serialise_batch_for_summariser`,
`allocate_short_refs`, `build_summary_message`, `ShortRef` and
its field, the `messages_since_compaction` counter the throttle
test reads) are `pub(super)`. Nothing new is `pub` —
`synthia_context::{SUMMARY_TAG, SummariseFn,
SummarizingContextManager, ToolCallArchive, ToolCallRecord,
CompactionRecord, CompactionLifecycle}` unchanged.

## Two defects caught and fixed mid-round

1. `build_summary_message` and `extract_tool_result_text` are
   the **only** callers of each other's neighbours; an early
   slice dropped one function's tail and another's closing
   brace. A **structural parity check** settled it: the new
   files' bodies (normalised for indentation, `pub(super)`, and
   the module-wiring lines) were diffed against
   `git show HEAD:…/summarizing.rs` — the only differences are
   the original `use` lines (now per-module) and rustfmt's
   re-wrapping of one signature. No logic lost, none
   duplicated.
2. The test module declaration was dropped from `mod.rs`, so
   the sibling suite silently did not compile (95 → 83 tests).
   Caught by comparing the count against the crate baseline —
   hence the report's rule: *a split round is not verified by
   "tests pass", it is verified by "the same tests ran".*

## Citation corrected (R117 follow-up)

R117's report and CHANGELOG entry attributed the deletion of
`the_cli_declares_the_env_vars_the_images_set` to a rule
"AGENTS.md §5" — AGENTS.md has no §5 (it ends at §4.2), so the
citation was false. Both documents now state the accurate
rationale (a source-text pin whose relative path broke on the
move) and no longer claim the `BindAddress::resolve` tests
preserve the env contract — they cover host/port *precedence*.

The contract that test claimed is now covered properly, in the
bin target where the `clap` attributes live:

```rust
let command = Args::command();
env_of = |id| command.get_arguments()
    .find(|arg| arg.get_id() == id).and_then(|arg| arg.get_env());
assert_eq!(env_of("host"), Some("SYNTHIA_HOST"));
assert_eq!(env_of("port"), Some("SYNTHIA_PORT"));
```

It asserts the **parsed CLI model**, not source text, and is
mutation-checked: changing the attribute to `SYNTHIA_HOST_TYPO`
fails it (verified, then reverted).

## Verification

- `synthia-context` lib **95/95** (identical to baseline, so
  every pre-existing test ran); `--features sqlite`
  **103/103**.
- `synthia-server` lib **402/402**; bin target **1/1** (new).
- Structural parity diff vs `HEAD` — clean.
- `cargo clippy --all-targets --all-features --tests --all
  -- -D warnings` — 0 warnings.
- `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p
  synthia-context` — clean (two intra-doc links qualified).
- `cargo +nightly fmt --all`; `make ci` **8/8**.
