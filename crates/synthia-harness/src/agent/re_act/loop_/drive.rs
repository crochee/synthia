//! The [`ReActLoop::drive`](super::ReActLoop::drive) orchestrator.
//!
//! [`drive`](super::ReActLoop::drive) is the harness's main
//! loop. This module owns the function and splits it into
//! named phases, each in its own helper, so the orchestrator
//! body reads as the state-machine list the harness's MVP
//! promise advertises.
//!
//! ## Phases
//!
//! | Phase helper | What it does |
//! |---|---|
//! | [`drive_setup`] | Session-start event + every hook's `OnAgentStart` + prepare messages + apply context |
//! | [`stamp_request_header`] | The R6-A typed event that records the model's identity + tool-list hash once per run, for replay determinism. |
//! | [`drive_one_iteration`] | One pass of the 5-step loop; dispatches on the [`IterationOutcome`] each step returns |
//! | [`drive_finalize`] | Post-loop: emit the max-iterations warning when the loop ran out, build the [`AgentOutput`], fan out every hook's `OnAgentEnd`. |
//!
//! ## Per-iteration helpers (kept short and flat)
//!
//! The helpers below are deliberately small, and the shape is held by
//! `make check-harness-shape` — no function in this crate's production
//! code over 100 lines or 4 nesting levels.
//!
//! `clippy.toml`'s older `cognitive-complexity-threshold = 20` is **not**
//! what keeps them that way: that lint is allow-by-default and, per its
//! own documentation, counts decision points only (no nesting, no loops,
//! no `?`), which the whole loop scores 8/20 against.
//!
//! - [`iteration_start_setup`] — bookkeeping + cancel check
//! - [`pre_sample_seams`] — drain steering / snapshot / hints
//! - [`sample_or_fail`] — provider call (logged on error)
//! - [`commit_assistant_and_step_end`] — history append + typed event
//! - [`handle_final_answer`] / [`handle_tool_calls`] — outcome-shape dispatch
//! - [`close_iteration`] — typed event + optional prune + steering drain
//!
//! ## Cross-cutting helpers
//!
//! - [`hooks_on_agent_start`] / [`hooks_on_agent_end`] — fan
//!   out the run-level hooks; failures become warning events.
//! - [`cancelled`] — the loop-wide "is the run cancelled?"
//!   check that also emits `SessionInterrupted` when the
//!   answer flips.
//!
//! These three live in [`super::drive_hooks`] — they are the
//! only helpers called from more than one phase, so they get
//! their own file rather than living with a single phase.

use synthia_context::AgentState;
use synthia_provider::{Message, ToolUse};
use tracing::{info, warn};

use super::{
    super::{input_text, is_length_stop},
    ReActLoop,
    drive_hooks::{cancelled, hooks_on_agent_end, hooks_on_agent_start},
    events::StepAction,
};
use crate::events::{AgentOutput, SessionEndReason, SystemEvent, WarningKind};

/// What the iteration body asked the orchestrator to do next.
pub(super) enum IterationOutcome {
    /// Continue the loop with the next iteration index.
    Continue,
    /// End the session with `Completed` (model returned text
    /// only, no follow-ups).
    Completed,
    /// End the session with the given reason (sample error,
    /// max-iterations, ordinary cancellation).
    Failed(SessionEndReason),
    /// End the session *now* and return this pre-built
    /// [`AgentOutput`] verbatim (cancelled-before-tool path).
    EarlyReturn(AgentOutput),
}

impl IterationOutcome {
    /// Whether the run has already been finalized *and reported*
    /// before the close phase runs.
    ///
    /// True only for [`IterationOutcome::EarlyReturn`]: that path
    /// emits `SessionEnded` and fans out `OnAgentEnd` inline, inside
    /// the iteration. `Completed` / `Failed` are terminal for the run
    /// but reach `SessionEnded` later, in `drive_finalize`, so their
    /// close phase is still properly ordered.
    fn already_finalized(&self) -> bool {
        matches!(self, Self::EarlyReturn(_))
    }
}

/// Mutable state carried across the loop iterations.
pub(super) struct SessionState {
    pub messages: Vec<Message>,
    pub agent_state: AgentState,
    pub end_reason: SessionEndReason,
    /// Names of the `SystemPrompt`-point hints already injected, so each
    /// one appends **at most once per session**.
    ///
    /// By name, not a single flag: one bool would let the first
    /// `SystemPrompt` hint to trigger permanently silence every other
    /// one for the rest of the run.
    pub system_hinted: std::collections::HashSet<String>,
    /// Set by a phase that could not continue but cannot return an
    /// outcome itself (currently only [`close_iteration`], which runs
    /// after the iteration's outcome is already fixed). The orchestrator
    /// turns it into `IterationOutcome::Failed` on the next tick, so no
    /// phase has to reshape the outcome enum to report a late failure.
    force_next_failure: Option<SessionEndReason>,
}

impl SessionState {
    /// State for a run that ended before its first iteration.
    fn failed(reason: SessionEndReason) -> Self {
        Self {
            messages: Vec::new(),
            agent_state: AgentState::with_window(0),
            end_reason: reason,
            system_hinted: std::collections::HashSet::new(),
            force_next_failure: None,
        }
    }
}

/// What the iteration body did with the LLM pass, plus the
/// bookkeeping the close phase needs.
///
/// `context_pruned` is `true` when the dispatch handler
/// already called `apply_context` inline (the follow-up
/// branch of [`handle_final_answer`]); the close phase then
/// skips its own prune to avoid double-budgeting.
struct IterationDispatch {
    outcome: IterationOutcome,
    truncated: bool,
    context_pruned: bool,
}

impl IterationDispatch {
    /// Keep looping; nothing pruned, nothing truncated.
    fn continuing() -> Self {
        Self {
            outcome: IterationOutcome::Continue,
            truncated: false,
            context_pruned: false,
        }
    }

    /// Keep looping, but the budget was already re-applied inline —
    /// the close phase must not prune again.
    fn pruned() -> Self {
        Self {
            outcome: IterationOutcome::Continue,
            truncated: false,
            context_pruned: true,
        }
    }

    /// Keep looping after a length-stopped tool batch was refused.
    fn truncated_batch() -> Self {
        Self {
            outcome: IterationOutcome::Continue,
            truncated: true,
            context_pruned: false,
        }
    }

    /// Stop now and return `output` verbatim.
    fn early(output: AgentOutput) -> Self {
        Self {
            outcome: IterationOutcome::EarlyReturn(output),
            truncated: false,
            context_pruned: false,
        }
    }

    /// Stop as `Completed`.
    fn completed() -> Self {
        Self {
            outcome: IterationOutcome::Completed,
            truncated: false,
            context_pruned: false,
        }
    }
}

/// Drive one session end-to-end.
///
/// The orchestrator reads as the four-step MVP harness the
/// architecture document advertises: setup, header, loop,
/// finalize. Every cross-cutting concern (hook fan-out,
/// cancel emission, request-header stamping) lives in its
/// own helper above.
#[tracing::instrument(
    name = "react_loop",
    level = "info",
    skip_all,
    fields(
        agent = %this.descriptor.name,
        max_iterations = this.max_iterations,
    ),
)]
pub(super) async fn drive(
    mut this: ReActLoop,
    input: crate::input::AgentInput,
) -> AgentOutput {
    info!(
        history_len = input.history.len(),
        "session start: preparing messages and dispatching LLM loop"
    );

    // Phase 1: setup. A failure here already carries the reason the run
    // must end with (a panicking context manager mid-rewrite), so there
    // is nothing to continue with.
    let mut session = match drive_setup(&mut this, &input).await {
        Ok(session) => session,
        Err(reason) => {
            return drive_finalize(this, SessionState::failed(reason), false)
                .await;
        }
    };

    // Phase 2: stamp the request header once per run.
    stamp_request_header(&this, &session.messages);

    // Phase 3: loop. The orchestrator only handles the four
    // outcomes; the per-iteration body lives in
    // `drive_one_iteration` below.
    let mut exhausted = true;
    for iteration in 0..this.max_iterations {
        let outcome =
            drive_one_iteration(&mut this, &mut session, iteration).await;
        // A phase that ran after the outcome was fixed (the close-phase
        // prune) can still have failed; that outweighs the outcome.
        let outcome = match session.force_next_failure.take() {
            Some(reason) => IterationOutcome::Failed(reason),
            None => outcome,
        };
        match outcome {
            IterationOutcome::Continue => {}
            IterationOutcome::Completed => {
                exhausted = false;
                session.end_reason = SessionEndReason::Completed;
                break;
            }
            IterationOutcome::Failed(reason) => {
                exhausted = false;
                session.end_reason = reason;
                break;
            }
            IterationOutcome::EarlyReturn(output) => return output,
        }
    }

    // Phase 4: finalize.
    drive_finalize(this, session, exhausted).await
}

/// The pre-loop setup phase: session-start event, every
/// hook's `OnAgentStart`, prepare messages, apply context.
async fn drive_setup(
    this: &mut ReActLoop,
    input: &crate::input::AgentInput,
) -> Result<SessionState, SessionEndReason> {
    this.sink.system(SystemEvent::SessionStarted {
        session_id: String::new(),
    });
    hooks_on_agent_start(this, &input_text(input)).await;
    let mut messages = this.prepare(input);
    let mut agent_state =
        AgentState::from_config(&this.provider.model_config());
    this.apply_context(&mut messages, &mut agent_state).await?;
    Ok(SessionState {
        messages,
        agent_state,
        end_reason: SessionEndReason::Completed,
        system_hinted: std::collections::HashSet::new(),
        force_next_failure: None,
    })
}

/// Stamp the R6-A typed event that records the model's
/// identity + tool-list hash once per run, for replay
/// determinism.
fn stamp_request_header(this: &ReActLoop, messages: &[Message]) {
    let cfg = this.provider.model_config();
    let tools = this.tool_definitions(messages);
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    names.hash(&mut h);
    this.emit_typed(synthia_session::request_header(
        this.provider.name(),
        &cfg.name,
        &format!("{:016x}", h.finish()),
        "initial",
    ));
}

/// One pass of the 5-step loop. Returns the outcome the
/// orchestrator dispatches on.
///
/// The body is a flat pipeline of five named phases — start,
/// pre-sample, sample + commit, dispatch on outcome shape,
/// close — each in its own helper below.
async fn drive_one_iteration(
    this: &mut ReActLoop,
    session: &mut SessionState,
    iteration: usize,
) -> IterationOutcome {
    let (outcome, action) = one_iteration_body(this, session, iteration).await;
    // Emitted here — once, on *every* exit path — rather than inside the
    // close phase. Phase 1 already emitted the matching
    // `iteration_start`, and the close phase is skipped for a run that
    // finalized itself, so gating the pair on it left an unpaired start
    // in the durable typed log (on that path and on the two early
    // returns below). Note this is the typed channel, not the event
    // stream: it cannot land after `SessionEnded` on a caller's stream.
    this.emit_iteration_end(iteration as u32, action);
    outcome
}

/// The iteration body, returning the outcome plus the action label its
/// `iteration_end` carries. Split from [`drive_one_iteration`] so the
/// typed start/end pair is emitted in exactly one place.
async fn one_iteration_body(
    this: &mut ReActLoop,
    session: &mut SessionState,
    iteration: usize,
) -> (IterationOutcome, &'static str) {
    // Phase 1: bookkeeping + cancel check.
    if let Some(outcome) = iteration_start_setup(this, session, iteration).await
    {
        return (outcome, "cancelled");
    }

    // Phase 2: pre-sample seams.
    pre_sample_seams(this, session, iteration).await;

    // Phase 3: sample + commit + step-end typed event.
    let outcome = match sample_or_fail(this, session, iteration).await {
        Ok(outcome) => outcome,
        Err(reason) => {
            return (IterationOutcome::Failed(reason), "sample_failed");
        }
    };
    commit_assistant_and_step_end(this, session, &outcome, iteration);

    // Phase 4: dispatch on the outcome shape.
    let dispatch = if outcome.has_tool_calls() {
        handle_tool_calls(this, session, &outcome, iteration).await
    } else {
        handle_final_answer(this, session, &outcome, iteration).await
    };

    // Phase 5: close the iteration — unless it already ended and
    // *reported* the run itself, in which case there is nothing left to
    // close: draining the inbox would append user turns to a finished
    // history and emit `SteeringInjected` after `SessionEnded`, the event
    // that tells consumers the run is over.
    let action = if dispatch.outcome.already_finalized() {
        // No tool ever ran; the close phase's labels would misdescribe it.
        "cancelled"
    } else if dispatch.truncated {
        "tools_truncated"
    } else {
        "tools_complete"
    };
    if !dispatch.outcome.already_finalized() {
        close_iteration(this, session, &dispatch).await;
    }
    (dispatch.outcome, action)
}

/// Phase 1 of [`drive_one_iteration`]: bookkeeping
/// (iteration_count, tracker, emit_iteration_start) + the
/// cancel-before-LLM check. Returns `Some(Failed)` if the
/// run was cancelled before sampling; `None` lets the
/// caller proceed.
async fn iteration_start_setup(
    this: &mut ReActLoop,
    session: &mut SessionState,
    iteration: usize,
) -> Option<IterationOutcome> {
    session.agent_state.iteration_count = iteration + 1;
    if let Err(panic) = crate::agent::re_act::catching_panics_sync(|| {
        this.steering.tracker.on_iteration(&session.agent_state);
    }) {
        warn!(panic = %panic, "tracker.on_iteration panicked");
    }
    this.emit_iteration_start(iteration as u32);
    if this.cancelled("cancelled before LLM call") {
        info!(
            iteration,
            "cancelled before LLM call; ending session with Cancelled"
        );
        return Some(IterationOutcome::Failed(SessionEndReason::Cancelled));
    }
    None
}

/// Phase 3a of [`drive_one_iteration`]: sample the provider.
///
/// The failure reason is **returned**, not logged-and-dropped: it is the
/// run's terminal `SessionEndReason`, and it carries whatever the
/// provider (or the panic guard around it) actually said. Swallowing it
/// here would report every sample failure — an HTTP error, a panicking
/// provider — as the same opaque string.
async fn sample_or_fail(
    this: &mut ReActLoop,
    session: &mut SessionState,
    iteration: usize,
) -> Result<super::SampleOutcome, SessionEndReason> {
    match this
        .sample_once(&session.messages, iteration, &mut session.agent_state)
        .await
    {
        Ok(out) => Ok(out),
        Err(reason) => {
            info!(
                iteration,
                ?reason,
                "sample_once returned Err; ending session"
            );
            Err(reason)
        }
    }
}

/// Phase 3b of [`drive_one_iteration`]: commit the
/// assistant turn + emit the step-end typed event with
/// the action label the outcome shape dictates.
fn commit_assistant_and_step_end(
    this: &mut ReActLoop,
    session: &mut SessionState,
    outcome: &super::SampleOutcome,
    iteration: usize,
) {
    this.commit_assistant(&mut session.messages, outcome);
    this.emit_step_end(
        0,
        iteration as u32,
        if outcome.has_tool_calls() {
            StepAction::ToolCall
        } else {
            StepAction::FinalAnswer
        },
    );
}

/// Phase 5 of [`drive_one_iteration`]: emit the
/// iteration-end typed event, optionally prune context
/// (the dispatch may have already done so), and drain any
/// steering that landed during the iteration.
async fn close_iteration(
    this: &mut ReActLoop,
    session: &mut SessionState,
    dispatch: &IterationDispatch,
) {
    // (`iteration_end` is emitted by `drive_one_iteration`, which runs on
    // every path including this one's skipped case.)
    // Only prune a run that will sample again; `Completed` is already
    // finishing. (An `EarlyReturn` never reaches this function — see
    // `drive_one_iteration`'s phase-5 gate.)
    if !dispatch.context_pruned
        && !dispatch.outcome.already_finalized()
        && !matches!(dispatch.outcome, IterationOutcome::Completed)
        && let Err(reason) = this
            .apply_context(&mut session.messages, &mut session.agent_state)
            .await
    {
        // The transcript may be half-rewritten; end the run reported
        // rather than sampling from it.
        session.force_next_failure = Some(reason);
        return;
    }
    this.drain_steering(&mut session.messages).await;
    this.snapshot_runtime_context(&mut session.messages);
    this.drain_steering(&mut session.messages).await;
}

/// The per-iteration setup right before sampling: drain
/// steering, snapshot the runtime context, inject hints, emit
/// progress + step-start events.
async fn pre_sample_seams(
    this: &mut ReActLoop,
    session: &mut SessionState,
    iteration: usize,
) {
    this.drain_steering(&mut session.messages).await;
    this.snapshot_runtime_context(&mut session.messages);
    this.inject_hints(
        &mut session.messages,
        &session.agent_state,
        &mut session.system_hinted,
        iteration,
    )
    .await;
    this.sink.system(SystemEvent::Progress {
        message: format!("LLM pass {iteration}"),
        step: iteration,
        total: this.max_iterations,
    });
    this.emit_step_start(0, iteration as u32);
}

/// Final-answer path: poll follow-ups at the would-be stop.
/// Empty → Completed. Non-empty → inject + apply_context +
/// continue (the apply_context inline is what gives the
/// follow-up test its "re-budget before the next sample"
/// invariant — see [`IterationDispatch::context_pruned`]).
async fn handle_final_answer(
    this: &mut ReActLoop,
    session: &mut SessionState,
    outcome: &super::SampleOutcome,
    iteration: usize,
) -> IterationDispatch {
    let follow_ups = if iteration + 1 < this.max_iterations {
        this.take_follow_ups().await
    } else {
        Vec::new()
    };
    if follow_ups.is_empty() {
        info!(
            iteration,
            assistant_text_len = outcome.assistant_text.len(),
            "no tool calls returned by model; ending session with Completed"
        );
        return IterationDispatch::completed();
    }
    info!(
        iteration,
        follow_up_count = follow_ups.len(),
        "follow-up messages received at would-be stop; continuing session"
    );
    this.append_injected(
        &mut session.messages,
        crate::events::SteeringSource::FollowUp,
        follow_ups,
    );
    if let Err(reason) = this
        .apply_context(&mut session.messages, &mut session.agent_state)
        .await
    {
        session.force_next_failure = Some(reason);
    }
    IterationDispatch::pruned()
}

/// Tool-call path: length-stop → fail the batch (no
/// execution). Else → cancel check, then `execute_tools`.
/// The cancelled-before-tool-execution case finalizes and
/// fans out `OnAgentEnd` immediately (the loop body returns
/// the `AgentOutput` rather than continuing).
async fn handle_tool_calls(
    this: &mut ReActLoop,
    session: &mut SessionState,
    outcome: &super::SampleOutcome,
    iteration: usize,
) -> IterationDispatch {
    let truncated = is_length_stop(&outcome.stop_reason);
    if truncated {
        return dispatch_truncated_batch(
            this,
            session,
            &outcome.tool_uses,
            iteration,
        )
        .await;
    }
    if !outcome.tool_uses.is_empty()
        && this.cancelled("cancelled before tool execution")
    {
        return dispatch_cancelled_before_tool(this, session, iteration).await;
    }
    dispatch_execute_tools(this, session, &outcome.tool_uses, iteration).await
}

/// Length-stop path: fail the batch without executing any
/// tool calls. Logs the standard warn and returns the
/// truncated-flagged dispatch.
async fn dispatch_truncated_batch(
    this: &mut ReActLoop,
    session: &mut SessionState,
    tool_uses: &[ToolUse],
    iteration: usize,
) -> IterationDispatch {
    warn!(
        iteration,
        tool_call_count = tool_uses.len(),
        "length stop with tool calls; failing the batch instead of executing"
    );
    this.fail_truncated_tool_batch(&mut session.messages, tool_uses);
    IterationDispatch::truncated_batch()
}

/// Cancel-before-tool path: finalize the run immediately and
/// fan out `OnAgentEnd`. The orchestrator returns the
/// pre-built `AgentOutput` via `EarlyReturn`.
async fn dispatch_cancelled_before_tool(
    this: &mut ReActLoop,
    session: &mut SessionState,
    iteration: usize,
) -> IterationDispatch {
    info!(
        iteration,
        "cancelled before tool execution; ending session with Cancelled"
    );
    let end_reason = SessionEndReason::Cancelled;
    let output = this.finalize(end_reason.clone(), &session.messages);
    hooks_on_agent_end(this, &output, &end_reason).await;
    IterationDispatch::early(output)
}

/// Default path: actually run the tools.
async fn dispatch_execute_tools(
    this: &mut ReActLoop,
    session: &mut SessionState,
    tool_uses: &[ToolUse],
    iteration: usize,
) -> IterationDispatch {
    info!(
        iteration,
        tool_call_count = tool_uses.len(),
        "step 4: executing tool calls returned by LLM"
    );
    this.execute_tools(
        &mut session.messages,
        tool_uses,
        &mut session.agent_state,
    )
    .await;
    IterationDispatch::continuing()
}

/// Post-loop finalize: emit the max-iterations warning when
/// the loop ran out, build the [`AgentOutput`], fan out every
/// hook's `OnAgentEnd`.
async fn drive_finalize(
    this: ReActLoop,
    session: SessionState,
    exhausted: bool,
) -> AgentOutput {
    let mut end_reason = session.end_reason;
    if exhausted {
        warn!("hit max_iterations ({})", this.max_iterations);
        this.sink.system(SystemEvent::Warning {
            kind: WarningKind::Loop,
            message: format!("hit max_iterations ({})", this.max_iterations),
            iteration: None,
        });
        end_reason = SessionEndReason::MaxIterations;
    }

    info!(
        end_reason = ?end_reason,
        history_len = session.messages.len(),
        "session end: finalize()"
    );
    warn_missing_structured_output(&this, &end_reason);
    let output = this.finalize(end_reason.clone(), &session.messages);
    hooks_on_agent_end(&this, &output, &end_reason).await;
    output
}

/// Observe a declared structured-output schema that the run never
/// satisfied.
///
/// `with_output_schema` only *offers* the model a `structured_output`
/// tool; it cannot force the call, and `FinalAnswer` (text, no tool
/// use) is a normal way for a run to end. A consumer that declared a
/// schema and reads a typed result therefore has, until now, no way to
/// tell "the model answered in prose instead" from "the model answered
/// with the shape I asked for" — the failure is silent.
///
/// The warning makes it observable. It is **not** fatal: prose is a
/// legitimate answer, and forcing a retry would change what "completed"
/// means for every existing agent.
///
/// Only a run that stopped on its own terms is checked. A cancelled or
/// errored run did not get the chance to submit, so warning there would
/// report a non-event.
fn warn_missing_structured_output(
    this: &ReActLoop,
    end_reason: &SessionEndReason,
) {
    if this.descriptor.output_schema.is_none() {
        return;
    }
    if !matches!(
        end_reason,
        SessionEndReason::Completed | SessionEndReason::MaxIterations
    ) {
        return;
    }
    if *this.structured_output.lock() {
        return;
    }
    this.sink.system(SystemEvent::Warning {
        kind: WarningKind::StructuredOutput,
        message: "the run declared an output schema but ended without a \
                  schema-valid `structured_output` submission; the typed \
                  result is absent"
            .to_string(),
        iteration: None,
    });
}

// Re-export the small helper as a method on ReActLoop so
// `this.cancelled(...)` reads naturally inside the
// per-iteration helpers.
impl ReActLoop {
    pub(super) fn cancelled(&self, reason: &str) -> bool {
        cancelled(self, reason)
    }
}
