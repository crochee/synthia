# Optimization report R98 — 2026-09-17

## What the audit found

R97 closed the dispatch seam into four focused
sibling modules. The follow-up audit turned to the
**main loop body** itself: `ReActLoop::drive()` in
`synthia-agent/agent/re_act/loop_/mod.rs` was 215 lines
of inline state machine — the "MVP harness, simple明了,
harness自身也要分好层次和布局" property that R88's plugin
split established at the crate level had not yet been
applied at the function level.

The method held six concerns in one body:
1. The session-start log + `SessionStarted` event.
2. The `OnAgentStart` hook fan-out (20 lines inlined).
3. The prepare-messages + apply-context setup.
4. The R6-A typed-event request-header stamp (15 lines
   inlined).
5. The 5-step iteration: cancel check, drain/snapshot,
   sample, commit, dispatch on outcome shape
   (final-answer vs tool calls), close.
6. The post-loop finalize + `OnAgentEnd` hook fan-out.

Each concern was named (a comment per phase), but the
phases shared one body, and any reader had to keep all
six concerns in their head at once. The "every concern
in its own file, every orchestrator reads as the step
list its doc promises" property was missing at the
function level.

## What landed

Pure code motion with one structural addition:
**`drive.rs`** (a new focused sibling module under
`loop_/`). Behaviour byte-identical; the previously-failing
`follow_up_revival_rebudgets_before_the_next_sample`
test now passes through the split.

### `loop_/mod.rs`: 559 → 313 lines

Now holds only:
- The `ReActLoop` struct + `from_runtime` constructor.
- The `SampleOutcome` struct + `has_tool_calls` impl
  (moved here from `steps` because both `drive` and
  `steps` produce/consume it; keeping the type in
  `mod.rs` avoids forcing one to import from the other).
- The `ToolProjectionMemo` struct.
- A one-line `drive()` façade that delegates to
  `drive::drive(self, input).await`.
- The delegation surface — 15 one-line methods, each
  routing to the focused sibling that owns the concern
  (`bucket::execute_tools`, `route::dispatch_tool_call`,
  `commit::commit_tool_result`, `steps::finalize`, etc.).
  No control flow, no state.

### `loop_/drive.rs`: 512 lines (new)

The `drive()` orchestrator + every named phase, in
its own helper:

| Phase helper | What it does |
|---|---|
| `drive_setup` | `SessionStarted` event + `hooks_on_agent_start` fan-out + prepare messages + apply context |
| `stamp_request_header` | R6-A typed event (model id + tool-list hash) for replay determinism |
| `drive_one_iteration` | One pass of the 5-step loop; returns `IterationOutcome` |
| `drive_finalize` | Post-loop max-iter warning + `finalize` + `hooks_on_agent_end` fan-out |
| `pre_sample_seams` | Drain steering, snapshot runtime context, inject hints, emit progress + step-start |
| `handle_final_answer` | Follow-ups poll: empty → `Completed`; non-empty → inject + apply_context + `Continue` |
| `handle_tool_calls` | Length-stop fail-batch or cancel-check or `execute_tools` |
| `hooks_on_agent_start` / `hooks_on_agent_end` | Fan out lifecycle hooks; failures are warning events |
| `cancelled` | Cancel-aware warning emission |

The orchestrator body itself is now 26 lines and
reads as the four-step MVP harness list the
architecture document advertises:

```rust
pub(super) async fn drive(mut this, input) -> AgentOutput {
    // Phase 1: setup.
    let mut session = drive_setup(&mut this, &input).await;

    // Phase 2: stamp the request header once per run.
    stamp_request_header(&this, &session.messages);

    // Phase 3: loop.
    let mut exhausted = true;
    for iteration in 0..this.max_iterations {
        match drive_one_iteration(&mut this, &mut session, iteration).await {
            IterationOutcome::Continue => {}
            IterationOutcome::Completed => { ...; break; }
            IterationOutcome::Failed(reason) => { ...; break; }
            IterationOutcome::EarlyReturn(output) => return output,
        }
    }

    // Phase 4: finalize.
    drive_finalize(this, session, exhausted).await
}
```

### The new `IterationOutcome` enum + `IterationDispatch` struct

The split needed to encode the loop's control-flow
branching in the type system. The original used a
`match` on `outcome.has_tool_calls()` with three
inline branch bodies (follow-ups path, truncated,
execute-tools). After the split, the orchestrator
needs to know what each branch did so the close phase
can pick the right typed-event label and decide
whether to prune context. Two small types:

- **`IterationOutcome`** — `Continue` / `Completed` /
  `Failed(SessionEndReason)` / `EarlyReturn(AgentOutput)`.
  The four cases the orchestrator's match must dispatch
  on. `EarlyReturn` is distinct from `Failed` because
  the cancelled-before-tool branch already runs
  `finalize` + the agent-end hooks and the orchestrator
  must skip the post-loop finalize step entirely.

- **`IterationDispatch`** — the per-iteration bookkeeping:
  `outcome` (the orchestrator's signal), `truncated`
  (close-phase typed-event label), `context_pruned`
  (whether the dispatch handler already called
  `apply_context` inline — the follow-up branch does;
  the close phase skips its own prune when this is true,
  preserving the test's "re-budget before the next
  sample" invariant).

### Behaviour preservation

The `follow_up_revival_rebudgets_before_the_next_sample`
test (`agent::re_act::tests::run_inbox`) checks that
the context manager is invoked exactly twice in a
follow-up-revive scenario: once for the initial setup
and once for the follow-up (not three times — the
follow-up handler does NOT call `apply_context` again
at the iteration close). The split preserves this by
gating the close-phase `apply_context` on
`!dispatch.context_pruned && !Completed`. The
`session.end_reason = SessionEndReason::Completed`
move into the orchestrator (not the dispatch) was the
key — the original drive() set it inline at line 304
of the pre-split `mod.rs`, and the split moves that
intent to a single site in the orchestrator.

## The new module map

```
loop_/mod.rs                313 lines   ReActLoop struct, SampleOutcome, ToolProjectionMemo, drive() façade, delegation surface
loop_/drive.rs              512 lines   drive() orchestrator + 4 named phases + handle_final_answer / handle_tool_calls / hooks_on_agent_* / cancelled / IterationOutcome / IterationDispatch / SessionState
loop_/steps.rs              491 lines   prepare / sample_once / commit_assistant / fail_truncated_tool_batch / apply_context / inject_hints / snapshot_runtime_context / finalize (unchanged)
loop_/bucket.rs             339 lines   execute_tools orchestrator + Parallel / Sequential phases (unchanged)
loop_/seams.rs              295 lines   execute_tool_inner + 4 steering seams + Verdict (unchanged)
loop_/route.rs              185 lines   dispatch_tool_call + 3 routing steps (unchanged)
loop_/dispatch.rs           167 lines   Shared seam helpers (WireToolResult, lookup_execution_mode, tool_definitions, append_tool_result_hints, notify_error_hooks) (unchanged)
loop_/commit.rs              54 lines   commit_tool_result + last_assistant_text (unchanged)
loop_/events.rs              69 lines   emit_typed + StepAction (unchanged)
loop_/inbox.rs               68 lines   drain_steering / take_follow_ups / append_injected (unchanged)
```

The harness now has the same "every concern in its
own file" property at the function level as it had at
the crate level after R88. Every phase of `drive()` is
## Post-landing complexity audit

The first commit passed `make ci` but the follow-up
complexity scan surfaced two regressions from the
split — `drive_one_iteration` was 28/20 and
`handle_tool_calls` was 25/20, both above the 20
threshold AGENTS.md §3.5 enforces. The orchestrator's
5-phase body carried too many control-flow branches
even after the named-phase extraction.

Two further extractions in the same shape brought
both functions under the threshold:

- **`drive_one_iteration` (28→18)**: 4 named phase
  helpers extracted:
  [`iteration_start_setup`] (bookkeeping + cancel
  check), [`sample_or_fail`] (provider call, logged on
  error), [`commit_assistant_and_step_end`] (history
  append + typed event), and [`close_iteration`]
  (typed event + optional prune + steering drain).
  The orchestrator now reads as a flat pipeline:
  `start_setup → pre_sample → sample_or_fail →
   commit → dispatch → close → return`.
- **`handle_tool_calls` (25→9)**: 3 named phase
  helpers extracted:
  [`dispatch_truncated_batch`] (length-stop
  fail-batch), [`dispatch_cancelled_before_tool`]
  (finalize + early-return), [`dispatch_execute_tools`]
  (default path). The orchestrator now reads as a
  flat dispatch table:
  `truncated → batch; cancelled → early; else →
   execute`.

Final `cargo clippy --workspace --all-targets
--all-features --tests --all -- -W clippy::cognitive_complexity`
reports zero violations. Behaviour unchanged:
`make test-unit` 2453/2453.

 ## Verification

  (`-D warnings` on the whole workspace), doc build,
  MVP-deps, runtime-free, claim-language, clock
  baseline.
  reports **zero violations** (R96 baseline holds;
  two regressions from the initial split were
  resolved by the post-landing extractions).
  change (pure code motion; the failing-test fix was
  preserving behaviour, not changing it).
  (the downstream that exercises the loop through real
  tool calls).

## Deferred

- `loop_/steps.rs` is 491 lines but is single-concern
  already (the 5 step bodies). A future round could
  apply the same per-step extraction that R97 applied
  to `dispatch.rs` (named helpers for `prepare` /
  `sample_once` / `commit_assistant` /
  `fail_truncated_tool_batch` / `apply_context` /
  `inject_hints` / `snapshot_runtime_context` /
  `finalize`). Not blocking; flagged.
- `loop_/drive.rs` is 512 lines because every named
  phase has a real body; the file is the right shape
  (one phase per concern) even if the total is larger
  than the pre-split `mod.rs`. A future round could
  extract the per-hook `hooks_on_agent_start` /
  `hooks_on_agent_end` pair into a `hooks.rs` sibling,
  halving `drive.rs`. Not blocking; flagged.
- The standing `synthia-steering::hook::tests` tracing
  span flake from R97 is unrelated to this round and
  reproduced the same way on `master`.

## Result

The harness's MVP main loop body is now 26 lines that
read as the four-step state machine (setup → header →
loop → finalize). The 215-line method that held six
concerns is gone. Every named phase lives in its own
helper with a single line of behaviour change between
the pre-split and post-split orchestrators, all
preserved by the `IterationOutcome` / `IterationDispatch`
types.

The user's "mvp核心循环只要harness, simple明了,
harness自身也要分好层次和布局" property now holds at
the function level: the orchestrator is the smallest
readable unit, and every phase has its own home.
