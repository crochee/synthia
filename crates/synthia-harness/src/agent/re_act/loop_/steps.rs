//! The loop's per-iteration step bodies.
//!
//! Every method here is one of the 5 steps (prepare → sample →
//! execute → repeat → finalize) or one of the cross-cutting
//! seams (context budget, hint injection, runtime-context
//! snapshot). The dispatch submodule owns tool dispatch; the
//! inbox submodule owns the run-inbox seams.

use std::sync::Arc;

use synthia_context::AgentState;
use synthia_core::Clock;
use synthia_provider::{
    CompletionRequest,
    CompletionResponse,
    Content,
    ContentPart,
    Message,
    Role,
    TextContent,
    ToolChoice,
    ToolUse,
};
use synthia_steering::{
    HintPriority,
    HookError,
    HookStage,
    InjectionPoint,
    run_hook,
};
use tracing::{debug, info};

use super::{ReActLoop, SampleOutcome};
use crate::{
    agent::re_act::{ChunkState, append_to_system_prompt},
    events::{AgentOutput, SessionEndReason, SystemEvent, WarningKind},
    prompt::RuntimeContext,
};

/// Seed the message history with the assembled system prompt,
/// any pre-existing `input.history` entries, and the new user
/// prompt.
pub(super) fn prepare(
    this: &ReActLoop,
    input: &crate::input::AgentInput,
) -> Vec<Message> {
    debug!(
        agent = %this.descriptor.name,
        history_len = input.history.len(),
        has_per_dispatch_ctx = input.prompt_context.is_some(),
        "step 1 prepare: assembling system prompt + history"
    );
    // The system prompt is byte-stable: volatile runtime facts live
    // in the RuntimeContext snapshot seam (see
    // `snapshot_runtime_context`), never in the system prompt —
    // that's the prompt-cache-stability contract.
    let assembled = assemble_system_prompt(this, input);
    let mut messages: Vec<Message> =
        Vec::with_capacity(input.history.len() + 2);
    if !assembled.is_empty() {
        messages.push(Message::system(assembled));
    }
    messages.extend(input.history.iter().cloned());
    messages.push(input.to_message());
    debug!(
        agent = %this.descriptor.name,
        final_messages_len = messages.len(),
        "step 1 prepare: messages vector ready for LLM"
    );
    messages
}

/// Assemble the dispatch's system prompt. The per-dispatch
/// manifest (set via `AgentInput::with_prompt_context`) wins
/// over the agent's snapshot manifest, so the server-side
/// dispatcher can rebuild it from the live registries on
/// every request without mutating the shared `Arc<dyn Agent>`.
fn assemble_system_prompt(
    this: &ReActLoop,
    input: &crate::input::AgentInput,
) -> String {
    let ctx: &crate::prompt::PromptContext = input
        .prompt_context
        .as_deref()
        .unwrap_or(&this.prompt_context);
    let assembled = ctx.assemble(&this.descriptor);
    debug!(
        agent = %this.descriptor.name,
        assembled_len = assembled.len(),
        skills = ctx.skills.len(),
        peer_agents = ctx.agents.len(),
        "step 1 prepare: system prompt assembled"
    );
    assembled
}

/// The streaming completion request one sampling pass sends.
///
/// Tool choice is `Auto` (the model decides whether to act) and the
/// provider's own defaults cover temperature and token cap — the
/// harness deliberately overrides neither, so a deployment's model
/// configuration is what the model actually gets.
fn completion_request(
    this: &ReActLoop,
    messages: &[Message],
) -> CompletionRequest {
    CompletionRequest {
        // The descriptor's `model_hint` wins, so per-agent model
        // selection (a judge routed through a stronger reasoning
        // model) reaches the provider.
        model: this.descriptor.model_hint.clone().unwrap_or_default(),
        messages: Arc::new(messages.to_vec()),
        tools: this.tool_definitions(messages),
        tool_choice: ToolChoice::Auto,
        temperature: None,
        max_tokens: None,
        stop_sequences: vec![],
        extra_body: None,
        cache_policy: None,
        replay_state: None,
    }
}

/// The one line an operator reads to see what the pass cost.
fn log_sample_result(
    iteration: usize,
    resp: &synthia_provider::CompletionResponse,
    outcome: &SampleOutcome,
) {
    info!(
        iteration,
        response_id = %resp.id,
        response_model = %resp.model,
        prompt_tokens = resp.usage.prompt_tokens,
        completion_tokens = resp.usage.completion_tokens,
        cached = resp.cached,
        stop_reason = ?resp.stop_reason,
        assistant_text_len = outcome.assistant_text.len(),
        tool_call_count = outcome.tool_uses.len(),
        "step 2 sample_once: streaming completion returned"
    );
}

/// Run one LLM sampling pass and translate every chunk to events.
///
/// Returns `Err(SessionEndReason)` for cancellation or fatal
/// stream errors. A successful return contains the assembled
/// assistant text + parts + tool uses.
///
/// The body is a flat pipeline — notify start hooks, stream
/// (cancellation-aware), then observe the outcome — each in
/// its own helper below.
pub(super) async fn sample_once(
    this: &ReActLoop,
    messages: &[Message],
    iteration: usize,
    agent_state: &mut AgentState,
) -> Result<SampleOutcome, SessionEndReason> {
    let req = completion_request(this, messages);
    info!(
        iteration,
        model = %req.model,
        messages_len = messages.len(),
        tool_definitions_len = req.tools.len(),
        "step 2 sample_once: dispatching streaming completion request to provider"
    );
    notify_provider_start_hooks(this, &req, iteration).await;

    let (resp, outcome, provider_elapsed) =
        match stream_completion(this, req, iteration).await {
            Ok(triple) => triple,
            Err(reason) => return Err(reason),
        };
    observe_provider_success(
        this,
        &resp,
        agent_state,
        iteration,
        provider_elapsed,
    )
    .await;
    log_sample_result(iteration, &resp, &outcome);
    Ok(outcome)
}

/// Fire every hook's `OnProviderStart` observation for `req`.
/// Hook-runner failures surface as warning events, never as
/// sample-fatal errors.
async fn notify_provider_start_hooks(
    this: &ReActLoop,
    req: &CompletionRequest,
    iteration: usize,
) {
    for hook in &this.steering.hooks {
        let req = req.clone();
        if let Err(err) =
            run_hook(&**hook, HookStage::OnProviderStart, move || {
                let hook = Arc::clone(hook);
                Box::pin(async move {
                    hook.on_provider_start(&req).await;
                    Ok::<_, HookError>(())
                })
            })
            .await
        {
            this.sink.system(SystemEvent::Warning {
                kind: WarningKind::Hook,
                message: err.to_string(),
                iteration: Some(iteration),
            });
        }
    }
}

/// Stream one completion to completion (pun intended): wire
/// the cancel token and the chunk-ingesting callback, await
/// the provider, then check for mid-stream cancellation
/// (which ends the session, not the turn).
///
/// On success it also finalizes the assembled
/// [`SampleOutcome`] from the chunk state (the state lives
/// and dies with the stream), stamping the provider's stop
/// reason; the caller handles usage observation.
async fn stream_completion(
    this: &ReActLoop,
    req: CompletionRequest,
    iteration: usize,
) -> Result<
    (CompletionResponse, SampleOutcome, std::time::Duration),
    SessionEndReason,
> {
    let cancel = Arc::clone(&this.cancel);
    let cb_cancel = cancel.clone();
    let chunk_state = ChunkState::default();
    let cb_state = chunk_state.clone();
    let cb_sink = this.sink.clone();
    let started = std::time::Instant::now();
    let result = crate::agent::re_act::catching_panics(
        this.provider.complete_with_stream(
            req,
            Some(cancel.clone()),
            Box::new(move |chunk| {
                if cb_cancel.is_cancelled() {
                    return;
                }
                cb_sink.ingest_chunk(&cb_state, chunk);
            }),
        ),
    )
    .await;
    let provider_elapsed = started.elapsed();

    // A provider is third-party code; a panic in it must become a
    // session-ending error rather than unwinding the run's task and
    // leaving the caller with a stream that ends without `SessionEnded`.
    let result = match result {
        Ok(inner) => inner,
        Err(message) => {
            super::dispatch::notify_error_hooks(this, "provider", &message)
                .await;
            this.sink.system(SystemEvent::Warning {
                kind: WarningKind::Hook,
                message: format!("LLM provider panicked: {message}"),
                iteration: Some(iteration),
            });
            return Err(SessionEndReason::Error(format!(
                "provider panicked: {message}"
            )));
        }
    };

    if cancel.is_cancelled() {
        info!(iteration, "step 2 sample_once: cancelled during LLM stream");
        this.sink.system(SystemEvent::SessionInterrupted {
            reason: "cancelled during LLM stream".to_string(),
        });
        return Err(SessionEndReason::Cancelled);
    }
    match result {
        Ok(resp) => {
            let mut outcome = this.sink.finalize_outcome(&chunk_state);
            outcome.stop_reason = resp.stop_reason.clone();
            Ok((resp, outcome, provider_elapsed))
        }
        Err(e) => {
            super::dispatch::notify_error_hooks(
                this,
                "provider",
                &e.to_string(),
            )
            .await;
            this.sink.system(SystemEvent::Warning {
                kind: WarningKind::Hook,
                message: format!("LLM stream error: {e}"),
                iteration: Some(iteration),
            });
            Err(SessionEndReason::Error(e.to_string()))
        }
    }
}

/// Observe a successful provider response: fold the usage
/// into the agent state and the typed event stream, hand it
/// to the steering tracker, and fire every hook's
/// `OnProviderEnd` with the elapsed time.
async fn observe_provider_success(
    this: &ReActLoop,
    resp: &CompletionResponse,
    agent_state: &mut AgentState,
    iteration: usize,
    provider_elapsed: std::time::Duration,
) {
    agent_state.add_token_usage(&resp.usage);
    this.emit_typed(synthia_session::usage(
        resp.usage.prompt_tokens,
        resp.usage.completion_tokens,
        resp.usage.total_tokens,
        resp.usage.reasoning_tokens,
        resp.usage.cache_read_tokens,
        resp.usage.cache_write_tokens,
    ));
    // Consumer-supplied (`Steering`); a panic here must not end the run
    // when every other steering surface degrades.
    if let Err(panic) = crate::agent::re_act::catching_panics_sync(|| {
        this.steering
            .tracker
            .on_llm_response(&resp.usage, agent_state);
    }) {
        tracing::warn!(panic = %panic, "tracker.on_llm_response panicked");
    }
    for hook in &this.steering.hooks {
        if let Err(err) = run_hook(&**hook, HookStage::OnProviderEnd, || {
            let hook = Arc::clone(hook);
            let resp = resp.clone();
            Box::pin(async move {
                hook.on_provider_end(&resp, provider_elapsed).await;
                Ok::<_, HookError>(())
            })
        })
        .await
        {
            this.sink.system(SystemEvent::Warning {
                kind: WarningKind::Hook,
                message: err.to_string(),
                iteration: Some(iteration),
            });
        }
    }
}

/// Append the assembled assistant turn to history.
///
/// The parts are committed **as produced**, not rebuilt from
/// `assistant_text` + `tool_uses`: the order is the provider's, and it
/// is load-bearing. Anthropic requires the signed `thinking` block that
/// preceded a tool call to be echoed back, in place, for interleaved
/// extended thinking to continue — and the provider crate's own
/// canonical fixture is `[Reasoning, Text]`, reasoning first.
///
/// An empty turn is the only thing skipped. A turn that is *only*
/// reasoning is not empty: it is the thinking block the next request
/// must carry, so it commits with no text and no tool call.
pub(super) fn commit_assistant(
    messages: &mut Vec<Message>,
    outcome: &SampleOutcome,
) {
    if outcome.parts.is_empty() {
        return;
    }
    messages.push(Message {
        role: Role::Assistant,
        content: Content::parts(coalesce_parts(&outcome.parts)),
        tool_call_id: None,
        name: None,
        tool_result_cleared_at: None,
    });
}

/// Collapse adjacent same-kind parts into one block each.
///
/// The assembler emits one part per **streamed delta**, which is right
/// for the wire — each delta is published as it arrives — and wrong for
/// history, which records *blocks*. A three-`thinking_delta` block would
/// otherwise commit as three `Reasoning` parts, and Anthropic turns each
/// into its own `ThinkingBlock`; it requires one thinking block per prior
/// assistant turn, so the request would be malformed. The same mechanism
/// used to split one answer into N `Text` parts, where it was a plain
/// regression against the single joined part history used to carry.
///
/// Runs are joined in order, so reasoning that precedes text (the shape
/// Anthropic's contract needs) still precedes it. Only *adjacent*
/// same-kind parts merge: a `Text` after a `ToolUse` is a different
/// block and stays separate.
///
/// Reasoning merges onto the first part of its run, which is where a
/// stamped signature already sits (the assembler stamps the trailing
/// un-signed run at finalize), so merging cannot drop one.
pub(super) fn coalesce_parts(parts: &[ContentPart]) -> Vec<ContentPart> {
    let mut out: Vec<ContentPart> = Vec::with_capacity(parts.len());
    for part in parts {
        match (out.last_mut(), part) {
            (Some(ContentPart::Text(prev)), ContentPart::Text(next)) => {
                prev.text.push_str(&next.text);
            }
            (
                Some(ContentPart::Reasoning(prev)),
                ContentPart::Reasoning(next),
            ) => {
                prev.text.push_str(&next.text);
                if prev.signature.is_none() {
                    prev.signature = next.signature.clone();
                }
            }
            _ => out.push(part.clone()),
        }
    }
    out
}

/// Fail every tool call in a batch that the model emitted under a
/// length stop, without executing any of them.
pub(super) fn fail_truncated_tool_batch(
    this: &ReActLoop,
    messages: &mut Vec<Message>,
    calls: &[ToolUse],
) {
    for call in calls {
        let error_text = format!(
            "Tool call \"{}\" was not executed: the response hit the \
             output token limit, so its arguments may be truncated. \
             Re-issue the tool call with complete arguments.",
            call.name
        );
        this.commit_tool_result(
            messages,
            super::WireToolResult {
                call_id: call.id.clone(),
                tool_name: call.name.clone(),
                content: vec![ContentPart::Text(TextContent {
                    text: error_text,
                    cache_control: None,
                })],
                is_error: true,
                metadata: serde_json::Map::new(),
                truncated_by: None,
            },
        );
    }
}

/// Run the configured [`ContextManager`] over the live history and
/// surface any pruning as a [`WarningKind::ContextCompaction`]
/// system event.
///
/// Returns `Err` only when the consumer-supplied manager panicked. The
/// failure is returned rather than logged-and-continued because the
/// manager rewrites `messages` **in place**: a panic mid-rewrite can
/// leave a half-truncated transcript, and that vec is what the next
/// provider call sends and the session log persists. Ending the run
/// reported is the only option that does not risk a corrupt history.
pub(super) async fn apply_context(
    this: &ReActLoop,
    messages: &mut Vec<Message>,
    state: &mut AgentState,
) -> Result<(), SessionEndReason> {
    // R29-Phase-K: hand the manager the file activity observed in
    // the iteration that just closed, then clear it. The manager is
    // consumer-supplied (`with_context_manager`), so a panic in it
    // would otherwise end the run unreported.
    let details_to_set = {
        let mut guard = this.touched.lock();
        if guard.read_files.is_empty() && guard.modified_files.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut *guard))
        }
    };
    let prepared = crate::agent::re_act::catching_panics(async {
        if let Some(details) = details_to_set {
            this.context_manager.set_compaction_details(details);
        }
        this.context_manager.prepare(messages, state).await;
    })
    .await;
    if let Err(message) = prepared {
        // Ended, not resumed: `prepare` rewrites `messages` **in place**,
        // and that vec is what the next provider call sends and the
        // session log persists. A panic mid-rewrite can leave a
        // half-truncated transcript, so carrying on would trade an
        // unreported death for a corrupt history. The caller turns this
        // into a reported `SessionEnded`.
        return Err(SessionEndReason::Error(format!(
            "context manager panicked: {message}"
        )));
    }
    if state.last_truncated {
        debug!(
            estimated_tokens = state.estimated_tokens,
            context_window = state.context_window,
            "context manager pruned history"
        );
        this.sink.system(SystemEvent::Warning {
            kind: WarningKind::ContextCompaction,
            message: format!(
                "context window pruned: ~{} of {} tokens in use",
                state.estimated_tokens, state.context_window
            ),
            iteration: None,
        });
    }
    Ok(())
}

/// Consult every hint and inject its reminder.
pub(super) async fn inject_hints(
    this: &ReActLoop,
    messages: &mut Vec<Message>,
    state: &AgentState,
    system_hinted: &mut std::collections::HashSet<String>,
    iteration: usize,
) {
    if this.steering.hints.is_empty() {
        return;
    }
    let mut reminders: Vec<String> = Vec::new();
    for hint in &this.steering.hints {
        // Skip the hints that belong to another injection point *before*
        // evaluating them. `AppendToToolResult` is applied at commit time
        // (`append_tool_result_hints`); consulting its trigger here would
        // run a consumer's `generate` only to discard the result, and
        // would make such a hint fatal on a path where it is not even
        // used.
        if matches!(
            hint.injection_point(),
            InjectionPoint::AppendToToolResult { .. }
        ) {
            continue;
        }
        // Hints are consumer-supplied (`Steering`), the same as the
        // guards and hooks the steering crate already isolates. A panic
        // here must not end the run: every other steering surface
        // degrades, so this one does too — the hint is skipped.
        let asked = crate::agent::re_act::catching_panics_sync(|| {
            hint.should_trigger(state).then(|| hint.generate(state))
        });
        let message = match asked {
            Ok(Some(message)) => message,
            Ok(None) => continue,
            Err(panic) => {
                tracing::warn!(
                    hint = hint.name(),
                    panic = %panic,
                    "hint panicked; skipping it"
                );
                continue;
            }
        };
        if message.priority == HintPriority::Low
            && state.context_utilization() >= 0.8
        {
            continue;
        }
        match hint.injection_point() {
            InjectionPoint::SystemPrompt => {
                // Session start only, as the point documents ("evaluated
                // once, at session start"). The system message is the
                // provider's cached prefix, so mutating it later would
                // invalidate the cache this crate keeps volatile facts
                // out of the system prompt to protect. Keyed by *name*,
                // so each hint gets its own chance rather than the first
                // to fire silencing the rest.
                //
                // The trigger is still consulted every iteration (the
                // `should_trigger` above), which is cheap and keeps this
                // match the only thing that varies by point; the append
                // is what is gated.
                if iteration == 0
                    && system_hinted.insert(hint.name().to_string())
                {
                    append_to_system_prompt(messages, &message.content);
                }
            }
            InjectionPoint::BeforeNextLlmCall | InjectionPoint::RecencyZone => {
                reminders.push(message.content);
            }
            InjectionPoint::AppendToToolResult { .. } => {
                // Unreachable: filtered above. Kept so adding an
                // `InjectionPoint` variant fails to compile here.
            }
        }
    }
    if reminders.is_empty() {
        return;
    }
    debug!(
        hint_count = reminders.len(),
        estimated_tokens = state.estimated_tokens,
        "injecting steering hints before next LLM call"
    );
    for content in reminders {
        messages.push(Message::user(format!("[reminder] {content}")));
    }
}

/// Render the runtime-context snapshot for this turn and append it
/// as a trailing user-role message when (and only when) the
/// rendered text differs from the last-appended snapshot.
pub(super) fn snapshot_runtime_context(
    this: &mut ReActLoop,
    messages: &mut Vec<Message>,
) {
    let now = this.clock.now();
    let runtime = RuntimeContext::from_runtime(
        &this.workspace_root.to_string_lossy(),
        now,
    );
    let rendered = runtime.render_snapshot();
    if this.last_runtime_snapshot.as_deref() == Some(rendered.as_str()) {
        debug!(
            history_len = messages.len(),
            "runtime-context snapshot unchanged; skipping append to preserve prompt cache"
        );
        return;
    }
    let user_message =
        Message::new(Role::User, Content::text(rendered.clone()));
    messages.push(user_message);
    this.last_runtime_snapshot = Some(rendered);
    debug!(
        history_len = messages.len(),
        "appended runtime-context snapshot before next LLM sample"
    );
}

/// Emit the terminal event and build the [`AgentOutput`].
pub(super) fn finalize(
    this: &ReActLoop,
    reason: SessionEndReason,
    messages: &[Message],
) -> AgentOutput {
    let final_message = super::commit::last_assistant_text(messages);
    this.sink.system(SystemEvent::SessionEnded { reason });
    AgentOutput { final_message }
}
