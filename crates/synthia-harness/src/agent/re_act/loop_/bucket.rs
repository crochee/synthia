//! The Parallel / Sequential bucketing of one LLM pass's tool calls.
//!
//! This module is the `execute_tools` orchestrator. One LLM
//! pass's tool-call batch is split by [`ExecutionMode`] into
//! the two buckets, run under their respective concurrency
//! rules, and the per-bucket outputs are parked into the
//! shared `outputs` slots this orchestrator owns. The
//! individual [`synthia_tool::ToolOutput`]s are committed to
//! the live history in LLM-call order at the end.
//!
//! This module knows nothing about model wiring, registries, or
//! hook emissions. It is a pure "given a list of calls and a
//! run, run them safely and commit the results" pipeline; the
//! per-call seams (restriction / hook veto / guard pipeline /
//! observation) live in [`super::seams`]; the per-call
//! dispatch (interceptor / registry / stream drain) lives in
//! [`super::route`].

use synthia_context::AgentState;
use synthia_provider::{Message, ToolUse};
use synthia_steering::tool_fingerprint;
use synthia_tool::{ExecutionMode, ToolOutput};
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};

use super::ReActLoop;
use crate::{
    agent::re_act::{parts_text, replace_text_parts},
    events::SystemEvent,
};

/// Run every tool call returned in one LLM pass.
///
/// Tools advertising [`ExecutionMode::Parallel`] (the default for
/// read-only / idempotent tools) run concurrently via
/// [`futures::future::join_all`]. Tools advertising
/// [`ExecutionMode::Sequential`] run strictly one-at-a-time and
/// abort the round on the first error.
///
/// The body is a flat pipeline of named phases — bucket, run
/// Parallel, run Sequential, commit — each in its own helper
/// below. The orchestrator reads as the four-step list the
/// module doc promises; the per-phase control flow (semaphore
/// permits, cancel checks, error aborts, steering transforms)
/// lives next to the phase it belongs to.
pub(super) async fn execute_tools(
    this: &ReActLoop,
    messages: &mut Vec<Message>,
    calls: &[ToolUse],
    agent_state: &mut AgentState,
) {
    if calls.is_empty() {
        return;
    }
    debug!(
        tool_call_count = calls.len(),
        "step 4 execute_tools: bucketing by ExecutionMode (Parallel vs Sequential)"
    );

    // 0. Record every call in the run state BEFORE dispatch.
    record_tool_fingerprints(calls, agent_state);
    let agent_state_ref: &AgentState = &*agent_state;

    // 1. Bucket by ExecutionMode.
    let (safe_indices, unsafe_indices, concurrency) =
        bucket_by_execution_mode(this, calls, agent_state_ref).await;

    // 2+3. Run both buckets; slots stay `None` for calls that
    // never ran (cancelled or aborted).
    let mut outputs: Vec<Option<ToolOutput>> =
        (0..calls.len()).map(|_| None).collect();
    run_parallel_bucket(
        this,
        calls,
        &safe_indices,
        concurrency,
        agent_state_ref,
        &mut outputs,
    )
    .await;
    run_sequential_bucket(
        this,
        calls,
        &unsafe_indices,
        agent_state_ref,
        &mut outputs,
    )
    .await;

    // 4. Commit ToolResults in LLM-call order.
    commit_all_results(this, messages, calls, agent_state_ref, outputs).await;
}

/// Phase 0 of [`execute_tools`]: record every call's
/// fingerprint in the run state BEFORE dispatch, so loop
/// detection sees the full batch even if an early call
/// aborts the round.
fn record_tool_fingerprints(calls: &[ToolUse], agent_state: &mut AgentState) {
    for call in calls {
        let fingerprint = tool_fingerprint(&call.name, &call.input);
        agent_state.record_tool_call(fingerprint);
    }
}

/// Phase 1 of [`execute_tools`]: split `calls` into the
/// Parallel bucket (indices of tools advertising
/// [`ExecutionMode::Parallel`]) and the Sequential bucket
/// (everything else), and clamp the steering tracker's
/// recommended concurrency to the Parallel bucket's size.
async fn bucket_by_execution_mode(
    this: &ReActLoop,
    calls: &[ToolUse],
    agent_state_ref: &AgentState,
) -> (Vec<usize>, Vec<usize>, usize) {
    let mut safe_indices: Vec<usize> = Vec::new();
    let mut unsafe_indices: Vec<usize> = Vec::new();
    for (idx, call) in calls.iter().enumerate() {
        let mode = super::dispatch::lookup_execution_mode(
            &this.tool_registry,
            &call.name,
        )
        .await;
        if mode == ExecutionMode::Parallel {
            safe_indices.push(idx);
        } else {
            unsafe_indices.push(idx);
        }
    }
    // The fourth consumer-supplied `Tracker` method; the other three are
    // guarded. A panic here falls back to the neutral bound
    // (`usize::MAX`, i.e. the batch's own size) rather than ending a run
    // the tracker was only advising.
    let recommended = crate::agent::re_act::catching_panics_sync(|| {
        this.steering
            .tracker
            .recommended_concurrency(agent_state_ref)
    });
    let recommended = match recommended {
        Ok(bound) => bound,
        Err(panic) => {
            warn!(
                panic = %panic,
                "tracker.recommended_concurrency panicked; using the \
                 batch's own bound"
            );
            usize::MAX
        }
    };
    let concurrency = recommended.min(safe_indices.len()).max(1);
    info!(
        tool_call_count = calls.len(),
        parallel_count = safe_indices.len(),
        sequential_count = unsafe_indices.len(),
        concurrency,
        "step 4 execute_tools: bucketed; running Parallel via join_all, Sequential serially"
    );
    (safe_indices, unsafe_indices, concurrency)
}

/// Phase 2 of [`execute_tools`]: run the Parallel bucket
/// concurrently under a semaphore capped at `concurrency`
/// permits, writing each result into its `outputs` slot.
async fn run_parallel_bucket(
    this: &ReActLoop,
    calls: &[ToolUse],
    safe_indices: &[usize],
    concurrency: usize,
    agent_state_ref: &AgentState,
    outputs: &mut [Option<ToolOutput>],
) {
    if safe_indices.is_empty() {
        return;
    }
    let semaphore = std::sync::Arc::new(Semaphore::new(concurrency));
    let futs = safe_indices.iter().map(|&idx| {
        let call = &calls[idx];
        let permit_source = std::sync::Arc::clone(&semaphore);
        async move {
            let _permit = permit_source.acquire_owned().await;
            (
                idx,
                super::seams::execute_tool_inner(this, call, agent_state_ref)
                    .await,
            )
        }
    });
    for (idx, out) in futures::future::join_all(futs).await {
        outputs[idx] = Some(out);
    }
}

/// Phase 3 of [`execute_tools`]: run the Sequential bucket
/// strictly in order. Stops at cancellation; aborts the rest
/// of the round after the first tool-reported error.
async fn run_sequential_bucket(
    this: &ReActLoop,
    calls: &[ToolUse],
    unsafe_indices: &[usize],
    agent_state_ref: &AgentState,
    outputs: &mut [Option<ToolOutput>],
) {
    let total_sequential = unsafe_indices.len();
    let mut sequential_aborted = false;
    for (seq_pos, idx) in unsafe_indices.iter().copied().enumerate() {
        match run_one_sequential(
            this,
            calls,
            idx,
            seq_pos,
            total_sequential,
            agent_state_ref,
            outputs,
        )
        .await
        {
            SequentialStep::Continue => {}
            SequentialStep::Cancelled => break,
            SequentialStep::Aborted => {
                sequential_aborted = true;
                break;
            }
        }
    }
    if sequential_aborted {
        debug!(
            tool_call_count = calls.len(),
            "step 4 execute_tools: Sequential round aborted after first error"
        );
    }
}

/// How one Sequential-bucket call ended: keep going, stop
/// because the run was cancelled, or stop because the tool
/// reported an error (the round aborts so corrective feedback
/// reaches the model before more side effects).
enum SequentialStep {
    Continue,
    Cancelled,
    Aborted,
}

/// One step of the Sequential bucket: emit the progress
/// event, run the call, park its output, and classify the
/// outcome for the caller's loop control.
async fn run_one_sequential(
    this: &ReActLoop,
    calls: &[ToolUse],
    idx: usize,
    seq_pos: usize,
    total_sequential: usize,
    agent_state_ref: &AgentState,
    outputs: &mut [Option<ToolOutput>],
) -> SequentialStep {
    if this.cancel.is_cancelled() {
        debug!(
            tool_call_count = calls.len(),
            remaining_sequential = total_sequential - seq_pos,
            "step 4 execute_tools: cancelled mid-Sequential batch"
        );
        return SequentialStep::Cancelled;
    }
    let call = &calls[idx];
    this.sink.system(SystemEvent::Progress {
        message: format!("Executing tool {}", call.name),
        step: 0,
        total: this.max_iterations,
    });
    let out =
        super::seams::execute_tool_inner(this, call, agent_state_ref).await;
    let abort = out.is_error.unwrap_or(false);
    outputs[idx] = Some(out);
    if abort {
        warn!(
            tool_name = %call.name,
            tool_use_id = call.id,
            "step 4 execute_tools: sequential tool reported error; aborting remaining sequential calls"
        );
        return SequentialStep::Aborted;
    }
    SequentialStep::Continue
}

/// Phase 4 of [`execute_tools`]: commit every result in
/// LLM-call order. Slots the buckets never filled get a
/// synthetic error; successful outputs pass through the
/// steering output-transformer and the AppendToToolResult
/// hints; errors commit verbatim (corrective feedback must
/// reach the model untouched).
async fn commit_all_results(
    this: &ReActLoop,
    messages: &mut Vec<Message>,
    calls: &[ToolUse],
    agent_state_ref: &AgentState,
    outputs: Vec<Option<ToolOutput>>,
) {
    let mut synthetic_count = 0usize;
    let mut error_count = 0usize;
    for (idx, call) in calls.iter().enumerate() {
        let mut output = outputs[idx].clone().unwrap_or_else(|| {
            ToolOutput::error("tool did not produce a result")
        });
        let is_error = output.is_error.unwrap_or(false);
        if outputs[idx].is_none() {
            synthetic_count += 1;
        }
        if is_error {
            error_count += 1;
        }
        // Steering: rewrite the textual projection of successful
        // outputs through the configured transformer, then append
        // any AppendToToolResult-point hints. Error outputs pass
        // through verbatim. Non-text parts (images) are preserved.
        if !is_error {
            let text = parts_text(&output.content);
            if !text.is_empty() {
                let mut text = this
                    .steering
                    .output_transformer
                    .transform(text, &call.name, agent_state_ref)
                    .await;
                text = super::dispatch::append_tool_result_hints(
                    this,
                    &call.name,
                    agent_state_ref,
                    text,
                );
                replace_text_parts(&mut output.content, text);
            }
        }
        let truncated_by = output
            .truncated_by
            .as_ref()
            .and_then(|t| serde_json::to_value(t).ok());
        super::commit::commit_tool_result(
            this,
            messages,
            super::dispatch::WireToolResult {
                call_id: call.id.clone(),
                tool_name: call.name.clone(),
                content: output.content,
                is_error,
                metadata: output.metadata,
                truncated_by,
            },
        );
    }
    info!(
        tool_call_count = calls.len(),
        error_count,
        synthetic_count,
        history_len = messages.len(),
        "step 4 execute_tools: all ToolResults committed to history"
    );
}
