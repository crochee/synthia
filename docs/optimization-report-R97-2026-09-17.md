# Optimization report R97 — 2026-09-17

## What the audit found

R92-R96 closed every production / test complexity
violation in the workspace. The follow-up audit turned to
the **layout** of the harness, where R88's plugin split
left a single 900-line module owning four distinct
concerns:

| File | Lines | Concerns held |
|---|---|---|
| `synthia-agent/agent/re_act/loop_/dispatch.rs` | 900 | 1. `execute_tools` orchestrator + Parallel / Sequential bucketing + commit-all-results. 2. `execute_tool_inner` orchestrator + the four steering seams (restriction, hook veto, guard pipeline, observation). 3. `dispatch_tool_call` orchestrator + the three routing steps (interceptor claim, registry lookup, stream drain). 4. `commit_tool_result` + `last_assistant_text`. |

The split had been *named* (each helper carries a phase
comment), but the four orchestrators + the seam controls
+ the bucket implementation + the commit helpers all
shared one file. Any reader opening `dispatch.rs` had to
keep four concerns in their head at once — the opposite
of the user's "MVP harness, every concern in its own
file" principle.

## What landed

Pure code motion, behaviour byte-identical:

- `synthia-agent/agent/re_act/loop_/dispatch.rs`: **900 → 167 lines**. Now holds only the three pieces the dispatch seam as a whole *shares*: `WireToolResult` (the wire payload produced by `bucket` and `steps`, consumed by `commit`), `lookup_execution_mode` + `tool_definitions` (the read-side memoised projection), and `append_tool_result_hints` + `notify_error_hooks` (the steering seams that span the dispatch step but don't belong to any one bucket).
- **New `bucket.rs` (339 lines)**: the `execute_tools` orchestrator + Parallel / Sequential bucketing, semaphore concurrency, the sequential-abort `SequentialStep` state machine, and the four-phase `commit_all_results`. The orchestrator reads as the four-step list its doc promises; per-phase control flow lives next to the phase.
- **New `seams.rs` (295 lines)**: the `execute_tool_inner` orchestrator + the four steering seams (restriction, hook veto, guard pipeline, observation) + the `Verdict<T>` enum shared with `route`. The orchestrator reads as the seam list each seam's doc promises.
- **New `route.rs` (185 lines)**: the `dispatch_tool_call` orchestrator + the three named routing steps (`try_interceptor` / `lookup_tool_entry` / `drain_tool_stream`). The orchestrator reads as that list.
- **New `commit.rs` (54 lines)**: `commit_tool_result` + `last_assistant_text`. Both are tiny but belong together (history-commit on the tool-result side, final-message projection on the finalize side) and together read as the "this is the live-history writeback layer" concept.

The `ReActLoop` struct (in `loop_/mod.rs`) keeps its 22
fields and the same delegation surface, but every
delegation now goes to the focused sibling module that
owns the concern:

| Method on `ReActLoop` | Delegated to |
|---|---|
| `prepare` / `sample_once` / `commit_assistant` / `fail_truncated_tool_batch` / `apply_context` / `inject_hints` / `snapshot_runtime_context` / `finalize` | `steps::*` (unchanged) |
| `tool_definitions` | `dispatch::tool_definitions` (unchanged) |
| `execute_tools` | `bucket::execute_tools` |
| `dispatch_tool_call` | `route::dispatch_tool_call` |
| `commit_tool_result` | `commit::commit_tool_result` |
| `drain_steering` / `take_follow_ups` / `append_injected` | `inbox::*` (unchanged) |

The old `execute_tool_inner` delegation in
`ReActLoop` (which used to forward `dispatch::execute_tool_inner`)
is deleted: `bucket.rs` calls `seams::execute_tool_inner`
directly, the indirection served no caller.

## The new module map

```
loop_/mod.rs              559 lines   ReActLoop struct, drive(), typed-event helpers, delegation surface
loop_/dispatch.rs         167 lines   Shared seam helpers + façade (WireToolResult, lookup_execution_mode, tool_definitions, append_tool_result_hints, notify_error_hooks)
loop_/bucket.rs           339 lines   execute_tools orchestrator + Parallel / Sequential phases
loop_/seams.rs            295 lines   execute_tool_inner orchestrator + four steering seams + Verdict<T>
loop_/route.rs            185 lines   dispatch_tool_call orchestrator + three routing steps
loop_/commit.rs            54 lines   commit_tool_result + last_assistant_text
loop_/steps.rs            491 lines   prepare / sample / commit_assistant / fail_truncated / apply_context / inject_hints / snapshot_runtime_context / finalize (unchanged)
loop_/events.rs            69 lines   emit_typed wrappers + StepAction (unchanged)
loop_/inbox.rs             68 lines   drain_steering / take_follow_ups / append_injected (unchanged)
```

Each orchestrator reads as the step list its module's
doc comment promises:

- `bucket::execute_tools` — the 5-line body lists steps 0-4.
- `seams::execute_tool_inner` — the 7-line body lists seams 0-3.
- `route::dispatch_tool_call` — the 5-line body lists the three routing steps.

The "harness is the MVP, every concern in its own file"
property is now true at the intra-file level too: every
loop subdirectory file is single-concern.

## Cross-module visibility

- `Verdict<T>` (defined in `seams`) is shared with `route`
  (which uses it for the `lookup_tool_entry` lookup) and
  with `bucket` (which never uses it directly but the seam
  returns it). Visibility widened from `pub(super)` to
  `pub(in crate::agent::re_act)` — the type is a private
  protocol of the dispatch seam and never reaches outside
  the `re_act` sub-tree.
- `WireToolResult` stays `pub(in crate::agent::re_act)` (it
  was already that wide) and is re-exported from `loop_/mod.rs`
  so `steps::fail_truncated_tool_batch` can build one without
  naming the dispatch module.
- The four new modules are `mod` (private); only
  `dispatch` and `steps` need to be re-exported because the
  crate's tests in `re_act/tests/` reference `dispatch::*`
  helpers directly.

## Verification

- `make ci` **7/7 green**: fmt-check, lint-rust
  (`-D warnings` on the whole workspace), doc build,
  MVP-deps, runtime-free, claim-language, clock baseline.
- `cargo clippy --workspace --all-targets --all-features
  --tests --all -- -W clippy::cognitive_complexity`
  reports **zero violations** (the R96 baseline holds).
- `make test-unit` **2453/2453** in 36.77 s (no count
  change — no test was added or removed; the split is
  pure code motion).
- `cargo test -p synthia-agent --lib` **256/256**.
- `cargo test -p synthia-delegation --lib` **66/66**
  (the loop's per-session downstream that exercises the
  dispatch path through real tool calls).
- `cargo +nightly fmt --all` clean.
- One transient flake: `synthia-steering::hook::tests::
  run_hook_opens_tracing_span_with_attributes` reported
  `expected current span name = "hook", got "|"` during
  the first `make test-unit` invocation. The flake is
  pre-existing (reproduced on `master` without my changes
  by re-running the full `make test-unit`); the test
  passes when the steering suite runs in isolation
  (`cargo test -p synthia-steering --lib` is **72/72**).
  It depends on tracing-subscriber state set up by an
  earlier test in the same binary — an ordering race that
  is independent of the R97 split. Logged for follow-up.

## Deferred

- The remaining 400-line+ files in `synthia-agent` are
  not single-file concerns (`strategy.rs` 556 lines,
  `builder.rs` 991 lines, `team.rs` 900 lines,
  `best_of_n.rs` 676 lines). They each have internal
  layout that is good for their concern, but a future
  round could surface the same kind of "split one
  orchestrator from its phases" treatment as this round
  applied to `dispatch.rs`. Not blocking; flagged.
- `stream.rs` (464 lines) is single-concern already
  (chunk ingestion) but the `handle_chunk` function is
  the largest in the file. A future round could apply
  the same pattern (named-phase helpers). Not blocking.
- The `synthia-steering::hook::tests` flake is unrelated
  to this round; the standalone reproduction confirms
  it's a pre-existing race.

## Result

`loop_/dispatch.rs` is no longer a 900-line dispatcher;
it is a 167-line façade that owns the shared seam
helpers and points at four focused sibling modules.
Each sibling holds one concern; each orchestrator reads
as the step list its module's doc promises. The
harness's "MVP, lego-style, single-concern per file"
property holds inside the file boundary now, not just
across the crate boundary.

Total agent loop lines: 2227 (up from 2126); the
increase is the four new module doc blocks + the
small `pub(in crate::agent::re_act)` re-export glue in
`mod.rs`. The trade is a cleaner mental model for every
reader who opens the harness.
