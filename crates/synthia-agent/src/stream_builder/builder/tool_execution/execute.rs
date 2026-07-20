//! Per-iteration tool execution with the L3-L5 recovery
//! cascade and the L1 truncation hook.
//!
//! Split out of [`super::run`] because the tool phase is
//! the most complex phase of the ReAct loop:
//!
//! 1. **Hook processing** — for each tool call requested
//!    by the LLM, fire the `before_tool` hook and honour
//!    its [`synthia_hook::ToolAction`] verdict
//!    (`Skip` / `Modify` / `Proceed` / `PendingConfirm`).
//!    Skip discards the call; Modify rewrites the name +
//!    input; Proceed / PendingConfirm / `Err` all pass the
//!    call through.
//! 2. **Execution** — call
//!    [`super::types::BuilderSteps::tool_execute`] and
//!    collect the [`ToolResult`]s.
//! 3. **Cascade** — on `Err(e)`, run the
//!    `run_recovery_cascade` (L3 fallback / L4
//!    auto-compact / L5 reset). The
//!    `RecoveryApplied` event is the only observer-visible
//!    signal that recovery fired; the recovered guidance
//!    is injected as a synthetic
//!    `ToolResult { is_error: true, .. }` so the next
//!    LLM call sees it.
//! 4. **L1 truncation** — apply
//!    `synthia_context::truncate::truncate_output` to
//!    every result (regardless of `is_error`) and emit
//!    `RecoveryApplied { level_number: 1, .. }` when
//!    truncation actually fires.
//! 5. **Persistence** — record each result in
//!    `ctx.recent_tool_results` and (when configured)
//!    send a `MemoryEvent::tool_executed` on the
//!    `memory_event_sender`.
//!
//! The helper does NOT yield events itself (Rust's
//! `async_stream` requires `yield` to live in the
//! `stream!` macro body). Instead it returns a
//! [`ToolExecuteOutcome`] enum that the caller pattern-
//! matches and yields the contained events for.
//!
//! [`ToolResult`]: crate::types::ToolResult

use std::sync::Arc;

use synthia_context::truncate::{
    DEFAULT_RETENTION,
    cleanup_tool_output_store_async,
    default_tool_output_dir,
};
use synthia_guardian::{ApprovalRequest, GuardianDecision, LoopDetectorSet};
use synthia_hook::{AgentContext, ToolAction};
use synthia_memory::types::MemoryEvent;
use synthia_provider::types::{SamplingResult, ToolUse};
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;

use super::types::ToolExecuteOutcome;
use crate::{
    config::AgentConfig,
    error_recovery::recovery_cascade::{RecoveryAction, run_recovery_cascade},
    events::{AgentEvent, SessionEndReason},
    loop_context::LoopContext,
    stream_builder::builder::types::BuilderSteps,
    types::ToolResult,
};

// ---------- Per-iteration entry point ----------

/// Run the per-iteration tool execution phase.
///
/// See [module-level documentation](self) for the full
/// phase breakdown.
#[allow(clippy::too_many_arguments)]
pub async fn execute_and_emit(
    steps: &mut BuilderSteps,
    ctx: &mut LoopContext,
    sampling: &SamplingResult,
    config: &AgentConfig,
    session_id: &str,
    cancel_token: CancellationToken,
    loop_detectors: &mut LoopDetectorSet,
    compaction_provider: Option<
        &dyn synthia_context::compaction::level1::CompactionProvider,
    >,
    memory_event_sender: Option<&Sender<MemoryEvent>>,
    agent_ctx: &AgentContext,
    output_bound: Option<&synthia_core::tool::OutputBound>,
) -> ToolExecuteOutcome {
    let mut events = Vec::new();

    // Phase 1: fire `before_tool` hooks and collect the
    // effective tool calls.
    let mut tool_calls_to_execute = collect_tool_calls(
        steps,
        agent_ctx,
        &sampling.tool_calls,
        &mut events,
        &steps.steering_channel,
        &mut ctx.forwarded_this_turn,
    )
    .await;

    // If the hooks filtered every call out, there is
    // nothing to execute. Surface the ToolCallStarted
    // events we already accumulated and continue.
    if tool_calls_to_execute.is_empty() {
        return ToolExecuteOutcome::Continue { events };
    }

    // Phase 1.5: Guardian permission gate. When a
    // `GuardianCoordinator` is configured, each tool call is mapped to
    // an [`ApprovalRequest`] and checked before execution. Denied calls
    // produce error `ToolResult`s (so the LLM sees the rationale) plus
    // a `GuardianWarning` event. Calls that need user confirmation are
    // forwarded to the orchestrator (Phase 2) which owns the actual
    // approval flow; they also emit a `GuardianWarning` per spec. When
    // a Guardian subagent review was initiated (`escalated == true`), a
    // `GuardianConfirmationRequest` event is emitted regardless of the
    // final decision. Approved calls proceed to Phase 2 unchanged.
    // Non-dangerous tools (no `ApprovalRequest` mapping) pass through
    // unchecked.
    //
    // Note on `GuardianConfirmationRequest` timing: the event is
    // emitted AFTER `coordinator.check(...).await` returns, based on
    // `outcome.escalated`, rather than before the review starts. This
    // is because the `GuardianCoordinator` does not have access to the
    // parent session's event channel during the review. The spec was
    // updated to match this implementation: the event reports that a
    // subagent review was initiated, surfaced after completion.
    let mut guardian_denied_results: Vec<ToolResult> = Vec::new();
    if let Some(coordinator) = steps.tool_execute.guardian_coordinator() {
        let subagent_factory = steps.tool_execute.subagent_factory();
        let mut approved_calls: Vec<ToolUse> = Vec::new();
        for tool_call in tool_calls_to_execute.drain(..) {
            let Some(request) = build_approval_request(&tool_call) else {
                approved_calls.push(tool_call);
                continue;
            };
            let outcome = coordinator
                .check(&request, &ctx.messages, &cancel_token, subagent_factory)
                .await;
            // Emit GuardianConfirmationRequest when a subagent review
            // was initiated (`escalated == true`), regardless of the
            // final decision. The event is reported after the review
            // completes because the coordinator does not have access
            // to the event channel during the review.
            if outcome.escalated {
                events.push(AgentEvent::GuardianConfirmationRequest {
                    tool_name: tool_call.name.clone(),
                    reason: "Guardian subagent review initiated".to_string(),
                });
            }
            match outcome.decision {
                GuardianDecision::Allow => approved_calls.push(tool_call),
                GuardianDecision::Deny { reason } => {
                    tracing::warn!(
                        tool = %tool_call.name,
                        reason = %reason,
                        escalated = outcome.escalated,
                        "Guardian denied tool call"
                    );
                    events.push(AgentEvent::GuardianWarning {
                        reason: format!(
                            "Guardian denied '{}': {}",
                            tool_call.name, reason
                        ),
                        iteration: ctx.iteration,
                    });
                    guardian_denied_results.push(ToolResult {
                        tool_name: tool_call.name.clone(),
                        tool_call_id: tool_call.id.clone(),
                        output: format!(
                            "Guardian denied this tool call: {reason}"
                        ),
                        is_error: true,
                    });
                }
                GuardianDecision::NeedUserConfirm { .. } => {
                    // Spec: emit GuardianWarning for NeedUserConfirm.
                    let reason = if outcome.escalated {
                        "Guardian subagent unavailable; user confirmation \
                         required"
                    } else {
                        "Guardian requires user confirmation"
                    };
                    tracing::info!(
                        tool = %tool_call.name,
                        escalated = outcome.escalated,
                        subagent_error = ?outcome.subagent_error,
                        "Guardian requests user confirmation"
                    );
                    events.push(AgentEvent::GuardianWarning {
                        reason: format!(
                            "Guardian requires user confirmation for '{}': \
                             {reason}",
                            tool_call.name
                        ),
                        iteration: ctx.iteration,
                    });
                    if steps.tool_execute.has_orchestrator() {
                        // Forward to the orchestrator so it can run its
                        // own approval flow (ApprovalService). The
                        // orchestrator's per-tool permission heuristic
                        // already classifies bash/write/apply_patch/
                        // multi_edit as `RequireConfirm`, so user
                        // confirmation will be requested through the
                        // configured `ApprovalService`.
                        approved_calls.push(tool_call);
                    } else {
                        // P6 (Distrust by Default): when no orchestrator
                        // is configured (registry-only execution path),
                        // there is no approval service to handle user
                        // confirmation. Deny the call rather than
                        // silently downgrading `NeedUserConfirm` to an
                        // execution.
                        let deny_reason = "user confirmation required but \
                             no approval service configured";
                        tracing::warn!(
                            tool = %tool_call.name,
                            reason = deny_reason,
                            escalated = outcome.escalated,
                            "NeedUserConfirm downgraded to Deny \
                             (no orchestrator)"
                        );
                        events.push(AgentEvent::GuardianWarning {
                            reason: format!(
                                "Guardian denied '{}' (no approval \
                                 service): {deny_reason}",
                                tool_call.name
                            ),
                            iteration: ctx.iteration,
                        });
                        guardian_denied_results.push(ToolResult {
                            tool_name: tool_call.name.clone(),
                            tool_call_id: tool_call.id.clone(),
                            output: format!(
                                "Guardian denied this tool call: \
                                 {deny_reason}"
                            ),
                            is_error: true,
                        });
                    }
                }
            }
        }
        tool_calls_to_execute = approved_calls;
    }

    // Phase 2 + 3: execute + cascade. When Guardian denied every call,
    // `tool_calls_to_execute` is empty and `execute` returns an empty
    // vec; the denied results are prepended below so they still flow
    // through the L1 truncation + event emission pipeline.
    let mut tool_results =
        match steps.tool_execute.execute(ctx, tool_calls_to_execute).await {
            Ok(results) => results,
            Err(e) => {
                // Handle edit conflicts specially — emit event and
                // treat as a non-recoverable tool error (no cascade).
                if let synthia_core::Error::EditConflict {
                    path,
                    original_hash,
                    current_hash,
                } = &e
                {
                    let tool_name = sampling
                        .tool_calls
                        .first()
                        .map(|c| c.name.clone())
                        .unwrap_or_else(|| "unknown".to_string());
                    let call_id = sampling
                        .tool_calls
                        .first()
                        .map(|c| c.id.clone())
                        .unwrap_or_else(|| "unknown".to_string());
                    let path_display = path.display().to_string();
                    events.push(AgentEvent::EditConflict {
                        tool_name: tool_name.clone(),
                        call_id: call_id.clone(),
                        path: path_display.clone(),
                        original_content_hash: *original_hash,
                        current_content_hash: *current_hash,
                    });
                    events.push(AgentEvent::ToolCallCompleted {
                        tool_name,
                        output: format!(
                            "Edit conflict detected on {}. \
                             File was modified since read. \
                             Original hash: {}, Current hash: {}",
                            path_display, original_hash, current_hash
                        ),
                        is_error: true,
                    });
                    return ToolExecuteOutcome::Continue { events };
                }

                // Capture the iteration BEFORE the cascade —
                // the L5 Reset arm clears `ctx.iteration = 0`
                // (a fresh conversation starts at 0) which
                // would otherwise report `iteration: 0` on
                // the RecoveryApplied event. We want the
                // iteration that *triggered* the recovery.
                let recovery_iteration = ctx.iteration;
                let tool_name_on_error = sampling
                    .tool_calls
                    .first()
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| "unknown".to_string());
                let tool_call_id_on_error = sampling
                    .tool_calls
                    .first()
                    .map(|c| c.id.clone())
                    .unwrap_or_else(|| "unknown".to_string());

                let steering_ref: Option<
                    &dyn crate::steering::SteeringChannel,
                > = steps.steering_channel.as_deref();
                let action = run_recovery_cascade(
                    &e.to_string(),
                    &tool_name_on_error,
                    ctx,
                    &mut steps.failure_tracker,
                    &steps.recovery,
                    config.context_token_budget.as_ref(),
                    compaction_provider,
                    loop_detectors,
                    steering_ref,
                    &steps.reset,
                )
                .await;
                match action {
                    RecoveryAction::Recovered { message, level } => {
                        events.push(AgentEvent::RecoveryApplied {
                            level_number: level,
                            tool_name: Some(tool_name_on_error.clone()),
                            message: message.clone(),
                            iteration: recovery_iteration,
                        });
                        tracing::info!(
                            level,
                            tool = %tool_name_on_error,
                            "Recovery cascade recovered from tool error; \
                             injecting fallback guidance as tool result"
                        );
                        // Inject the fallback message as a tool
                        // result with `is_error: true` so the LLM
                        // sees the cascade's guidance on the next
                        // iteration. The ToolResult carries the
                        // same `tool_name` as the failing call so
                        // the LLM can correlate the guidance with
                        // the prior request.
                        vec![ToolResult {
                            tool_name: tool_name_on_error,
                            tool_call_id: tool_call_id_on_error,
                            output: message,
                            is_error: true,
                        }]
                    }
                    RecoveryAction::FailFast(reason) => {
                        tracing::error!(
                            tool = %tool_name_on_error,
                            reason = %reason,
                            "Recovery cascade exhausted; entering fail-fast"
                        );
                        ctx.set_end_reason(SessionEndReason::Error(reason));
                        let end_reason =
                            ctx.end_reason.clone().unwrap_or_else(|| {
                                SessionEndReason::Error("Unknown".to_string())
                            });
                        return ToolExecuteOutcome::Terminate {
                            events: vec![AgentEvent::SessionEnded {
                                reason: end_reason,
                            }],
                        };
                    }
                    RecoveryAction::Escalate => {
                        // Cascade no longer produces Escalate
                        // (L5 is wired in), but keep the arm
                        // explicit for forward compatibility.
                        tracing::error!(
                            tool = %tool_name_on_error,
                            "Recovery cascade escalated (unexpected); \
                             entering fail-fast"
                        );
                        ctx.set_end_reason(SessionEndReason::Error(
                            "Recovery cascade escalated".to_string(),
                        ));
                        let end_reason =
                            ctx.end_reason.clone().unwrap_or_else(|| {
                                SessionEndReason::Error("Unknown".to_string())
                            });
                        return ToolExecuteOutcome::Terminate {
                            events: vec![AgentEvent::SessionEnded {
                                reason: end_reason,
                            }],
                        };
                    }
                }
            }
        };

    // Prepend Guardian-denied results so they flow through the same
    // L1 truncation + event emission pipeline as executed results.
    if !guardian_denied_results.is_empty() {
        let mut combined = guardian_denied_results;
        combined.extend(tool_results);
        tool_results = combined;
    }

    // Phase 4 + 5: L1 output binding, emit `ToolCallCompleted`,
    // record into `ctx.recent_tool_results`, and send the
    // `MemoryEvent::tool_executed` event.
    //
    // When `output_bound` is provided, we use
    // `OutputBound::bind()` to truncate; otherwise the output
    // passes through unchanged.
    let mut any_truncated = false;
    for result in &tool_results {
        let bound = output_bound.map(|ob| ob.bind(&result.output));
        let effective_output = if let Some(ref br) = bound
            && br.truncated
        {
            any_truncated = true;
            events.push(AgentEvent::RecoveryApplied {
                level_number: 1,
                tool_name: Some(result.tool_name.clone()),
                message: format!(
                    "Truncated tool output ({} -> {} bytes)",
                    br.original_bytes, br.output_bytes
                ),
                iteration: ctx.iteration,
            });
            bound.unwrap().output
        } else if let Some(br) = bound {
            br.output
        } else {
            result.output.clone()
        };
        events.push(AgentEvent::ToolCallCompleted {
            tool_name: result.tool_name.clone(),
            output: effective_output.clone(),
            is_error: result.is_error,
        });
        ctx.add_tool_result(
            result.tool_name.clone(),
            result.tool_call_id.clone(),
            effective_output.clone(),
            !result.is_error,
        );

        // Dispatch PostToolUse via UnifiedHookDispatcher so the
        // Layer 2 LoopDetector can record the call.
        let post_tool_event = synthia_hook::HookEvent::PostToolUse(
            synthia_hook::outcome::PostToolUsePayload {
                session_id: session_id.to_string(),
                tool_name: result.tool_name.clone(),
                input: serde_json::Value::Null,
                output: serde_json::Value::String(
                    effective_output.chars().take(200).collect(),
                ),
            },
        );
        let post_tool_outcome =
            steps.hook_dispatcher.dispatch(&post_tool_event).await;
        if post_tool_outcome.is_denied() {
            tracing::warn!(
                tool = %result.tool_name,
                "PostToolUse hook denied (non-blocking, logged only)"
            );
        }
        if let Some(sender) = memory_event_sender
            && let Err(e) = sender
                .send(MemoryEvent::tool_executed(
                    session_id.to_string(),
                    result.tool_name.clone(),
                    !result.is_error,
                ))
                .await
        {
            tracing::warn!(
                error = %e,
                "Failed to send tool_executed memory event"
            );
        }
    }

    // Touch `cancel_token` so the unused-variable lint
    // does not fire — we currently do not cancel mid-
    // tool-execution, but the signature is forward-
    // looking (the cascade could become cancellable).
    let _ = cancel_token;

    if any_truncated {
        let base_dir = default_tool_output_dir();
        tokio::spawn(async move {
            let _ =
                cleanup_tool_output_store_async(&base_dir, DEFAULT_RETENTION)
                    .await;
        });
    }

    ToolExecuteOutcome::Continue { events }
}

// ---------- Per-iteration helpers ----------

/// Fire `before_tool` for each requested tool call and
/// collect the effective calls to execute.
///
/// Emits one `AgentEvent::ToolCallStarted` per request
/// (including the ones the hook will Skip) into
/// `events_out`. The returned [`ToolUse`] list contains
/// the modified or passthrough calls that the executor
/// should actually run.
async fn collect_tool_calls(
    steps: &BuilderSteps,
    agent_ctx: &AgentContext,
    tool_calls: &[ToolUse],
    events_out: &mut Vec<AgentEvent>,
    steering_channel: &Option<Arc<dyn crate::steering::SteeringChannel>>,
    forwarded_this_turn: &mut usize,
) -> Vec<ToolUse> {
    let mut tool_calls_to_execute: Vec<ToolUse> = Vec::new();
    for tool_call in tool_calls {
        events_out.push(AgentEvent::ToolCallStarted {
            tool_name: tool_call.name.clone(),
            input: tool_call.input.clone(),
        });

        // Phase 1a: dispatch PreToolUse via UnifiedHookDispatcher.
        // Deny → skip the tool; ForwardToMainAgent → inject into
        // steering and continue; Allow → proceed to old hook.
        let pre_tool_event = synthia_hook::HookEvent::PreToolUse(
            synthia_hook::outcome::PreToolUsePayload {
                session_id: agent_ctx.session_id.clone(),
                tool_name: tool_call.name.clone(),
                input: tool_call.input.clone(),
            },
        );
        let pre_tool_outcome =
            steps.hook_dispatcher.dispatch(&pre_tool_event).await;
        match &pre_tool_outcome {
            synthia_hook::HookOutcome::Deny { reason } => {
                tracing::warn!(
                    tool = %tool_call.name,
                    reason = %reason,
                    "PreToolUse hook denied via UnifiedHookDispatcher"
                );
                // Dispatch PreMessageDrop: the tool call message is about
                // to be dropped because the hook denied it.
                let pre_drop_event = synthia_hook::HookEvent::PreMessageDrop(
                    synthia_hook::outcome::PreMessageDropPayload {
                        session_id: agent_ctx.session_id.clone(),
                        reason: synthia_hook::outcome::DropReason::HookDenied,
                    },
                );
                steps.hook_dispatcher.dispatch(&pre_drop_event).await;
                continue;
            }
            synthia_hook::HookOutcome::ForwardToMainAgent { hint } => {
                if *forwarded_this_turn < crate::steering::FORWARDED_RATE_LIMIT
                    && let Some(channel) = steering_channel
                {
                    channel
                        .send(crate::steering::SteeringMessage::forwarded(hint))
                        .await;
                    *forwarded_this_turn += 1;
                }
                // ForwardToMainAgent does NOT block the tool call.
            }
            synthia_hook::HookOutcome::Allow => {}
        }

        // Phase 1b: fire legacy before_tool hooks for Modify support.
        // The fire_before_tool can return ToolAction::Modify
        // which rewrites the tool input — this capability is not yet
        // available in the new HookOutcome model.
        let call_json =
            serde_json::to_string(&tool_call.input).unwrap_or_default();
        let call_value: serde_json::Value =
            serde_json::from_str(&call_json).unwrap_or_default();
        match steps
            .hooks
            .get_registry()
            .fire_before_tool(agent_ctx, &call_value)
            .await
        {
            Ok(ToolAction::Skip) => {
                // Skip this tool
                continue;
            }
            Ok(ToolAction::Modify(new_input)) => {
                let modified_name = new_input
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .unwrap_or_else(|| tool_call.name.clone());
                let modified_input = new_input
                    .get("input")
                    .cloned()
                    .unwrap_or_else(|| tool_call.input.clone());
                tool_calls_to_execute.push(ToolUse {
                    id: tool_call.id.clone(),
                    name: modified_name,
                    input: modified_input,
                });
                tracing::debug!(
                    tool = %tool_call.name,
                    "Hook modified tool input"
                );
            }
            Ok(ToolAction::Proceed)
            | Ok(ToolAction::PendingConfirm { .. })
            | Err(_) => {
                tool_calls_to_execute.push(tool_call.clone());
            }
        }
    }
    tool_calls_to_execute
}

/// Build an [`ApprovalRequest`] from a [`ToolUse`] for Guardian review.
///
/// Returns `None` for non-dangerous tools (read-only / search tools),
/// which bypass the Guardian entirely. Dangerous tools are mapped to
/// the closest-matching `ApprovalRequest` variant so that
/// [`SimpleGuardian::assess_risk`] can score them:
///
/// - `bash` → [`ApprovalRequest::shell`] (risk based on command content)
/// - `apply_patch` → [`ApprovalRequest::apply_patch`] (risk based on patch)
/// - `write` / `multi_edit` → [`ApprovalRequest::apply_patch`] (content
///   serialized as patch text; risk 40 for safe content)
fn build_approval_request(tool_call: &ToolUse) -> Option<ApprovalRequest> {
    match tool_call.name.as_str() {
        "bash" => {
            let command = tool_call
                .input
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Some(ApprovalRequest::shell(
                &tool_call.id,
                vec![command],
                "/",
                None,
            ))
        }
        "apply_patch" => {
            let patch = tool_call
                .input
                .get("patch")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Some(ApprovalRequest::apply_patch(
                &tool_call.id,
                "/",
                vec![],
                0,
                patch,
            ))
        }
        "write" | "multi_edit" => {
            // Serialize the input as patch content so risk scoring can
            // scan for dangerous patterns (rm -rf, sudo, chmod 777).
            let content =
                serde_json::to_string(&tool_call.input).unwrap_or_default();
            Some(ApprovalRequest::apply_patch(
                &tool_call.id,
                "/",
                vec![],
                0,
                content,
            ))
        }
        _ => None,
    }
}
