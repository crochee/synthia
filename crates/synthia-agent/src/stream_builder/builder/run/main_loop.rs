use std::{pin::Pin, sync::Arc};

use async_stream::stream;
use parking_lot::Mutex;
use synthia_context::{
    compact_context_tool::COMPACT_CONTEXT_TOOL_NAME,
    prefix_tracker::{PrefixStabilityEvent, PrefixTracker},
    truncate::{
        DEFAULT_RETENTION,
        cleanup_tool_output_store_async,
        default_tool_output_dir,
    },
};
use synthia_core::tool::{
    fragment::FragmentContext,
    rollout::{ChangeType, FileChange},
};
use synthia_provider::types::Message;
use synthia_telemetry::{
    CompactionAnalyticsAttempt,
    CompactionTrigger,
    SpanContext,
};

use super::{
    super::types::{BuilderSteps, StreamBuilder},
    helpers::{emit_turn_event, handle_hook_outcome},
};
use crate::{
    config::AgentRunConfig,
    control::CompletedTask,
    events::{
        AgentEvent,
        HookEvent,
        SAMPLE_COMPLETED,
        SESSION_ENDED,
        SystemEvent,
        TOOL_CALL_ISSUED,
        TOOL_RESULT_RECEIVED,
        TURN_COMPLETED,
        TURN_FAILED,
        TURN_STARTED,
        WarningKind,
    },
    loop_context::LoopContext,
    turn::{TurnStatus, TurnTask},
};

/// Format a completed background sub-agent task as a structured
/// `<task>` XML notification suitable for injection into the parent
/// conversation context.
fn format_background_task_notification(task: &CompletedTask) -> String {
    let (state, inner_tag) = match task.status {
        crate::registry::instance::AgentStatus::Completed => {
            ("completed", "task_result")
        }
        _ => ("error", "task_error"),
    };
    format!(
        "<task id=\"{}\" state=\"{}\">\n<summary>Background task {}: {}</summary>\n<{}>\n{}\n</{}>\n</task>",
        task.agent_id,
        state,
        state,
        task.agent_id,
        inner_tag,
        task.output,
        inner_tag
    )
}

impl StreamBuilder {
    /// Internal entry: takes session inputs by value so
    /// the produced `stream!` block is `'static` and the
    /// returned `Pin<Box<dyn Stream>>` can outlive
    /// `&self`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn run_with_steps(
        &self,
        run_config: AgentRunConfig,
        mut steps: BuilderSteps,
        initial_state: Option<(Vec<Message>, usize)>,
        prefix_tracker: Arc<Mutex<PrefixTracker>>,
        on_prefix_event: Option<
            Arc<dyn Fn(PrefixStabilityEvent) + Send + Sync + 'static>,
        >,
        // Optional callback invoked after each LLM call with the
        // token usage from the provider response. Used for OTel
        // cache token metrics export (KV cache hit ratio).
        on_usage: Option<
            Arc<dyn Fn(synthia_provider::TokenUsage) + Send + Sync + 'static>,
        >,
        system_snapshot: Vec<u8>,
    ) -> Pin<Box<dyn futures::Stream<Item = AgentEvent> + Send>> {
        let AgentRunConfig {
            provider,
            tool_registry,
            hook_registry: _,
            model_router: _,
            user_id,
            session_id,
            input,
            config,
            context_assembler: _,
            session_store,
            steering_channel: _steering_channel_direct,
            session_input_queue,
            cancel_token,
            memory_event_sender: _memory_event_sender_direct,
            agent_control,
            fork_policy: _,
            // L4 auto-compaction provider, surfaced as a
            // separate binding so the cascade can take
            // `&dyn CompactionProvider` without going
            // through the `ProviderConfig` field on
            // `AgentConfig` (which is a different type and
            // serves a different purpose: it is the
            // *static* config used to deserialize a config
            // from disk).
            compaction_provider: compaction_provider_runtime,
            // Factory for creating real child sessions. Not yet
            // consumed by the main loop; carried through for
            // sub-agent tooling in later tasks.
            subagent_session_factory: _subagent_session_factory,
            // Tool orchestrator is consumed by `StepToolExecute` rather
            // than the main loop directly.
            tool_orchestrator: _,
            approval_service: _,
            sandbox_manager: _,
            // Guardian coordinator is consumed by `StepToolExecute`.
            guardian_coordinator: _,
            extension_manager: _,
            extension_registry,
            rollout_tracker,
            interceptor_chain,
            loop_services,
        } = run_config;

        // ── Registry-First service resolution ──
        // When the InterceptorChain is available, prefer accessing
        // migrated services through it (Registry-First path). Fall
        // back to the direct run_config fields when the chain is
        // absent (legacy path).
        let memory_event_sender = interceptor_chain
            .as_ref()
            .and_then(|c| c.memory_event_sender().cloned())
            .or(_memory_event_sender_direct);
        let _steering_channel: Option<
            Arc<dyn crate::steering::SteeringChannel>,
        > = interceptor_chain
            .as_ref()
            .and_then(|c| c.steering_channel().cloned())
            .or(_steering_channel_direct);

        let session_id_clone = session_id.clone();

        Box::pin(stream! {
            yield AgentEvent::System(SystemEvent::SessionStarted {
                session_id: session_id_clone.clone(),
            });

            // Dispatch SessionStart hook event via UnifiedHookDispatcher
            let session_start_event = synthia_hook::HookEvent::SessionStart(
                synthia_hook::outcome::SessionStartPayload {
                    session_id: session_id_clone.clone(),
                },
            );
            steps.hook_dispatcher.dispatch(&session_start_event).await;

            if let Err(e) = session_store.ensure_session_dir(&user_id, &session_id_clone) {
                tracing::warn!(session_id = %session_id_clone, error = %e, "Failed to ensure session directory");
            }

            // Best-effort cleanup of stale offloaded tool output.
            let cleanup_base_dir = default_tool_output_dir();
            tokio::spawn(async move {
                if let Err(e) = cleanup_tool_output_store_async(&cleanup_base_dir, DEFAULT_RETENTION).await {
                    tracing::warn!(error = %e, "Failed to clean up stale tool output");
                }
            });

            let span_ctx = SpanContext::new(&session_id_clone);

            // H4 fix: restore ALL four resumable fields (iteration,
            // end_reason, cumulative_tokens, context_token_limit) from
            // persisted session metadata BEFORE seeding messages.
            //
            // Previously only the last two were restored, leaving
            // `iteration` at 0 and `end_reason` at None — meaning a
            // resumed session that had hit `max_iterations` would
            // silently restart iteration counting and run forever,
            // and a session that ended due to a fatal `end_reason`
            // would lose that signal on resume.
            //
            // `LoopContext::from_metadata` initializes `messages` to
            // an empty Vec, so seeding happens afterward to avoid
            // being clobbered.
            let metadata = session_store
                .load_metadata(&user_id, &session_id_clone)
                .ok();
            let mut ctx = match metadata.as_ref() {
                Some(m) => LoopContext::from_metadata(
                    session_id_clone.clone(),
                    span_ctx,
                    m,
                ),
                None => LoopContext::new(session_id_clone.clone(), span_ctx),
            };

            // ── RegistrationScope ──
            // When an ExtensionRegistry is available, create a
            // RegistrationScope from its ToolRegistry. Any tools
            // registered during this session under the scope's token
            // will be automatically unregistered when the LoopContext
            // (and thus the scope) is dropped at session end.
            if let Some(ref ext_reg) = extension_registry {
                let scope = ext_reg.tool_registry().create_session_scope();
                tracing::info!(
                    session_id = %session_id_clone,
                    token = scope.token().0,
                    "Created session RegistrationScope"
                );
                ctx.registration_scope = Some(scope);
            }

            // If resuming from checkpoint, restore state; otherwise seed with input message.
            // Only treat initial_state as "resume" if there are actual messages or iteration > 0.
            super::super::iteration::seed_initial_messages(
                &mut ctx,
                initial_state.as_ref(),
                &input,
            );

            // ── FragmentRegistry system prompt ──
            // When the extension_registry is available, render active
            // fragments and inject them as a system message. This
            // replaces the legacy ContextAssembler system prompt path
            // when fragments are registered. If no fragments are
            // registered, fall through to the existing context
            // assembler system prompt (inserted by `seed_initial_messages`
            // via the `ContextAssembler::build_system_prompt()` path).
            if let Some(ref ext_reg) = extension_registry {
                let frag_reg = ext_reg.fragment_registry();
                let frag_ctx = FragmentContext::new(
                    &session_id_clone,
                    &user_id,
                );
                let rendered = frag_reg.render_active(&frag_ctx).await;
                if !rendered.is_empty() {
                    let system_content: String = rendered
                        .iter()
                        .map(|(name, content)| {
                            format!("## {name}\n{content}")
                        })
                        .collect::<Vec<String>>()
                        .join("\n\n");
                    // Check if a system message already exists and
                    // replace it, otherwise prepend.
                    if let Some(first) = ctx.messages.first_mut() {
                        if first.role == synthia_provider::Role::System {
                            first.content =
                                synthia_provider::types::Content::text(
                                    system_content,
                                );
                        }
                    } else {
                        ctx.messages.insert(
                            0,
                            Message::system(system_content),
                        );
                    }
                }
            }

            // Initialize `context_token_limit` from the configured token
            // budget when it was not restored from session metadata. Without
            // this, `LoopContext::token_ratio()` returns 0.0 (the field is
            // `None`), and the 80% auto-trigger threshold for
            // `compact_context` can never fire.
            if ctx.context_token_limit.is_none()
                && let Some(budget) = config.context_token_budget.as_ref()
            {
                ctx.context_token_limit = Some(budget.hard_limit);
            }

            let mut loop_detectors = synthia_guardian::LoopDetectorSet::new();

            // Track whether (and when) the in-loop self-reflection fired
            // during the current run. The end-of-session reflection below
            // uses this to avoid double-reflection: if the last in-loop
            // reflection already happened at the final iteration, we skip
            // the end-of-session call.
            let mut last_reflect_iteration: Option<usize> = None;
            // Tracks the last iteration at which a compaction fired
            // (LLM-driven or auto-triggered). Used for observability;
            // same-iteration dedup is handled by the
            // `llm_compact_called_this_iter` flag passed to
            // `maybe_auto_trigger_compact_context`.
            let mut last_compact_iteration: Option<usize> = None;

            // ── OperationContext ──────────────
            // Creates an operation context with deadline derived
            // from `session_wall_clock_timeout` and the cancel
            // token. Used for deadline checks between turns
            // and goal status evaluation.
            let _op_ctx = synthia_service::context::OperationContext::for_session(
                session_id_clone.clone(),
                user_id.clone(),
                "main-loop",
            );

            while !ctx.should_stop_with_timeout(
                config.max_iterations,
                config.session_wall_clock_timeout,
            ) {
                // ── Deadline check ────
                // If the operation context has expired, break.
                // The existing should_stop_with_timeout already
                // handles wall-clock timeout; this provides an
                // additional early-exit path via OperationContext.
                if _op_ctx.is_expired() {
                    tracing::info!(
                        session_id = %session_id_clone,
                        "OperationContext deadline expired"
                    );
                    // The main should_stop_with_timeout will
                    // handle the actual break; this is an
                    // additional observation point.
                }

                // ── Goal status check ──
                // If goal is achieved or blocked, break.
                if let Some(services) = loop_services.get()
                    && let Some(ref tracker) = services.goal_tracker
                {
                    let status = tracker.status().await;
                    if status == synthia_service::goal::GoalStatus::Achieved
                        || status
                            == synthia_service::goal::GoalStatus::Blocked
                    {
                        tracing::info!(
                            session_id = %session_id_clone,
                            ?status,
                            "Goal tracker reports terminal status, ending session"
                        );
                        ctx.set_end_reason(
                            crate::events::SessionEndReason::Completed,
                        );
                        break;
                    }
                }
                // Drain steering channel at start of iteration
                for ev in super::super::iteration::drain_steering(
                    &mut ctx,
                    session_input_queue.as_ref(),
                    &user_id,
                    &session_id_clone,
                ).await {
                    yield ev;
                }

                // Check for completed background sub-agents and inject
                // structured notifications into the parent context.
                if let Some(ref control) = agent_control {
                    let completed = control.check_completed().await;
                    for task in completed {
                        let synthetic_msg = Message::user(format_background_task_notification(&task));
                        let state = match task.status {
                            crate::registry::instance::AgentStatus::Completed => "completed",
                            _ => "error",
                        };
                        ctx.messages.push(synthetic_msg);
                        yield AgentEvent::Hook(HookEvent::Message {
                            priority: 0,
                            message: format!(
                                "Background task {} {}",
                                task.agent_id, state
                            ),
                        });
                    }
                }

                ctx.increment_iteration();
                // Reset the forwarded-this-turn counter for the new iteration.
                ctx.forwarded_this_turn = 0;
                let turn_id = ctx.assign_new_turn_id();
                // Create the `turn.start` span for this iteration. The
                // span's parent is auto-inherited from
                // `tracing::Span::current()` (the `session.start` span
                // established by `wrap_output_with_otel` in
                // `agent::otel_context`). The guard is held for the
                // entire turn iteration; on drop (via `continue` /
                // `break` / `return` / fall-through / panic) the span
                // is ended. Failure paths call `record_error` before
                // exiting the scope to set the span status to `Error`
                // and record an OTel exception event.
                #[cfg(feature = "otel")]
                let mut turn_span_guard =
                    crate::agent::otel_context::TurnSpanGuard::create(
                        &turn_id,
                        ctx.iteration,
                    );
                let mut current_turn = TurnTask::new(&session_id_clone);
                current_turn.id = turn_id;
                emit_turn_event(
                    session_store.event_store(),
                    &session_store,
                    &user_id,
                    &session_id_clone,
                    TURN_STARTED,
                    turn_id,
                    ctx.iteration,
                    serde_json::Value::Null,
                )
                .await;

                if cancel_token.is_cancelled() {
                    // Dispatch PreMessageDrop hook event: messages are about
                    // to be dropped due to cancellation.
                    let pre_drop_event = synthia_hook::HookEvent::PreMessageDrop(
                        synthia_hook::outcome::PreMessageDropPayload {
                            session_id: session_id_clone.clone(),
                            reason: synthia_hook::outcome::DropReason::Cancelled,
                        },
                    );
                    steps.hook_dispatcher.dispatch(&pre_drop_event).await;

                    // Fail any in-flight tool calls before exiting the
                    // loop so the interruption is observable as terminal
                    // `ToolCallCompleted` events and persisted to the
                    // session JSONL + `ctx.recent_tool_results`. Without
                    // this, tool calls that were mid-flight when the
                    // cancellation signal arrived would be left dangling
                    // (no terminal event), violating P8 (Transform,
                    // Never Lose) and P5 (Recency Anchoring).
                    let interrupted = steps.tool_execute.fail_interrupted_tools();
                    for (tool_name, call_id) in interrupted {
                        yield AgentEvent::Model(synthia_provider::ContentPart::ToolResult(
                            synthia_provider::ToolResult {
                                tool_use_id: call_id.clone(),
                                content: vec![synthia_provider::ContentPart::Text(
                                    synthia_provider::TextContent {
                                        text: "Tool execution interrupted".to_string(),
                                        cache_control: None,
                                    },
                                )],
                                structured_content: None,
                                is_error: Some(true),
                            },
                        ));
                        ctx.add_tool_result(
                            tool_name.clone(),
                            call_id.clone(),
                            "Tool execution interrupted".to_string(),
                            false,
                        );
                        emit_turn_event(
                            session_store.event_store(),
                            &session_store,
                            &user_id,
                            &session_id_clone,
                            TOOL_RESULT_RECEIVED,
                            turn_id,
                            ctx.iteration,
                            serde_json::json!({
                                "tool_name": tool_name,
                                "call_id": call_id,
                                "output": "Tool execution interrupted",
                                "is_error": true,
                            }),
                        )
                        .await;
                    }
                    ctx.set_end_reason(crate::events::SessionEndReason::Cancelled);
                    current_turn.fail_with("cancelled");
                    #[cfg(feature = "otel")]
                    turn_span_guard.record_error("TurnError", "cancelled");
                    emit_turn_event(
                        session_store.event_store(),
                        &session_store,
                        &user_id,
                        &session_id_clone,
                        TURN_FAILED,
                        turn_id,
                        ctx.iteration,
                        serde_json::json!({"reason": "cancelled"}),
                    )
                    .await;
                    emit_turn_event(
                        session_store.event_store(),
                        &session_store,
                        &user_id,
                        &session_id_clone,
                        SESSION_ENDED,
                        turn_id,
                        ctx.iteration,
                        serde_json::json!({"reason": "Cancelled"}),
                    )
                    .await;
                    yield AgentEvent::System(SystemEvent::SessionEnded {
                        reason: crate::events::SessionEndReason::Cancelled,
                    });
                    return;
                }

                // IterationStarted is no longer emitted on the wire
                // (Phase 2 of simplify-agent-event-stream); internal
                // iteration tracking is preserved via ctx.iteration.

                // Dispatch PreCompact hook event before checking compact
                // step. The hook can observe (but not veto) the pending
                // compaction check.
                let pre_compact_event = synthia_hook::HookEvent::PreCompact(
                    synthia_hook::outcome::PreCompactPayload {
                        session_id: ctx.session_id.clone(),
                        token_count: ctx.cumulative_tokens,
                    },
                );
                steps.hook_dispatcher.dispatch(&pre_compact_event).await;

                let compact_outcome = super::super::iteration::do_compact_step(
                    &steps.compact,
                    &mut ctx,
                    &config,
                );
                match compact_outcome {
                    super::super::iteration::CompactOutcome::None => {}
                    super::super::iteration::CompactOutcome::Warning => {
                        let threshold = config
                            .context_token_budget
                            .as_ref()
                            .map(|b| b.hard_limit)
                            .unwrap_or(0);
                        yield AgentEvent::System(SystemEvent::Warning {
                            kind: WarningKind::TokenBudget,
                            message: format!(
                                "Token budget warning: {}/{}",
                                ctx.cumulative_tokens, threshold
                            ),
                            iteration: Some(ctx.iteration),
                        });
                    }
                    super::super::iteration::CompactOutcome::MustCompact { old_tokens, new_tokens } => {
                        // Dispatch PostCompact hook event after compaction.
                        let post_compact_event = synthia_hook::HookEvent::PostCompact(
                            synthia_hook::outcome::PostCompactPayload {
                                session_id: ctx.session_id.clone(),
                                token_count: new_tokens,
                            },
                        );
                        steps.hook_dispatcher.dispatch(&post_compact_event).await;

                        yield AgentEvent::System(SystemEvent::Warning {
                            kind: WarningKind::ContextCompaction,
                            message: format!(
                                "Compacted {} -> {} tokens",
                                old_tokens, new_tokens
                            ),
                            iteration: Some(ctx.iteration),
                        });
                        let threshold = config
                            .context_token_budget
                            .as_ref()
                            .map(|b| b.hard_limit)
                            .unwrap_or(0);
                        yield AgentEvent::System(SystemEvent::Warning {
                            kind: WarningKind::TokenBudget,
                            message: format!(
                                "must_compact: {}/{}",
                                ctx.cumulative_tokens, threshold
                            ),
                            iteration: Some(ctx.iteration),
                        });
                        current_turn.fail_with("must_compact");
                        #[cfg(feature = "otel")]
                        turn_span_guard.record_error("TurnError", "must_compact");
                        emit_turn_event(
                            session_store.event_store(),
                            &session_store,
                            &user_id,
                            &session_id_clone,
                            TURN_FAILED,
                            turn_id,
                            ctx.iteration,
                            serde_json::json!({"reason": "must_compact"}),
                        )
                        .await;
                        continue;
                    }
                }

                let tool_definitions = super::super::iteration::build_tool_definitions(
                    &tool_registry,
                    ctx.cumulative_tokens,
                )
                .await;

                // LlmRequestStarted is no longer emitted on the wire
                // (Phase 2 of simplify-agent-event-stream); the LLM call
                // itself is observed via its ModelDone/SamplingResult.

                let agent_ctx = super::super::iteration::prepare_agent_ctx(&ctx);
                let before_llm_event = synthia_hook::HookEvent::UserPromptSubmit(
                    synthia_hook::outcome::UserPromptSubmitPayload {
                        session_id: ctx.session_id.clone(),
                        prompt_summary: String::new(),
                    },
                );
                let before_llm_outcome = steps.hook_dispatcher.dispatch(&before_llm_event).await;
                handle_hook_outcome(
                    &before_llm_outcome,
                    &steps.steering_channel,
                    &mut ctx.forwarded_this_turn,
                ).await;

                // Dispatch PreResponse hook event: the LLM is about to
                // generate a response. This is semantically distinct from
                // UserPromptSubmit (which fires when the user input arrives)
                // — PreResponse fires right before the sampling call.
                let pre_response_event = synthia_hook::HookEvent::PreResponse(
                    synthia_hook::outcome::PreResponsePayload {
                        session_id: ctx.session_id.clone(),
                    },
                );
                let pre_response_outcome = steps.hook_dispatcher.dispatch(&pre_response_event).await;
                handle_hook_outcome(
                    &pre_response_outcome,
                    &steps.steering_channel,
                    &mut ctx.forwarded_this_turn,
                ).await;

                // Capture prefix snapshot BEFORE the LLM call: system +
                // tools + messages. All three participate in the hash so
                // that any change (system prompt edited, tool added, or
                // messages prefix mutated) is detected by the rolling
                // stability window (D4).
                let tools_schema_bytes =
                    PrefixTracker::canonical_tools_schema_bytes(&tool_definitions);
                let messages_prefix_bytes =
                    PrefixTracker::canonical_messages_prefix_bytes(&ctx.messages);
                prefix_tracker.lock().record_pre(
                    &system_snapshot,
                    &tools_schema_bytes,
                    &messages_prefix_bytes,
                    ctx.iteration as u64,
                );

                let sample_outcome = super::super::iteration::sample_llm_and_cascade(
                    &mut steps,
                    provider.clone(),
                    &mut ctx,
                    tool_definitions,
                    cancel_token.clone(),
                    &config,
                    &mut loop_detectors,
                    compaction_provider_runtime.as_deref(),
                )
                .await;

                match sample_outcome {
                    super::super::iteration::LlmSampleOutcome::Continue { events } => {
                        for ev in events { yield ev; }
                        current_turn.fail_with("sample_cascade_continue");
                        #[cfg(feature = "otel")]
                        turn_span_guard.record_error("TurnError", "sample_cascade_continue");
                        emit_turn_event(
                            session_store.event_store(),
                            &session_store,
                            &user_id,
                            &session_id_clone,
                            TURN_FAILED,
                            turn_id,
                            ctx.iteration,
                            serde_json::json!({"reason": "sample_cascade_continue"}),
                        )
                        .await;
                        continue;
                    }
                    super::super::iteration::LlmSampleOutcome::Terminate { events } => {
                        for ev in events { yield ev; }
                        current_turn.fail_with("sample_cascade_terminate");
                        #[cfg(feature = "otel")]
                        turn_span_guard.record_error("TurnError", "sample_cascade_terminate");
                        emit_turn_event(
                            session_store.event_store(),
                            &session_store,
                            &user_id,
                            &session_id_clone,
                            TURN_FAILED,
                            turn_id,
                            ctx.iteration,
                            serde_json::json!({"reason": "sample_cascade_terminate"}),
                        )
                        .await;
                        emit_turn_event(
                            session_store.event_store(),
                            &session_store,
                            &user_id,
                            &session_id_clone,
                            SESSION_ENDED,
                            turn_id,
                            ctx.iteration,
                            serde_json::json!({"reason": "SampleCascadeTerminate"}),
                        )
                        .await;
                        return;
                    }
                    super::super::iteration::LlmSampleOutcome::Done { sampling, events: pre_events } => {
                        current_turn.transition_to(TurnStatus::Sampling);
                        emit_turn_event(
                            session_store.event_store(),
                            &session_store,
                            &user_id,
                            &session_id_clone,
                            SAMPLE_COMPLETED,
                            turn_id,
                            ctx.iteration,
                            serde_json::json!({
                                "text": sampling.text,
                                "tool_call_count": sampling.tool_calls.len(),
                            }),
                        )
                        .await;
                        for ev in pre_events { yield ev; }

                        // Detect an LLM-driven `self_reflect` call so we can
                        // reset the auto-trigger counter and avoid double-
                        // reflection in the same iteration.
                        let llm_self_reflect = sampling
                            .tool_calls
                            .iter()
                            .any(|c| {
                                c.name
                                    == synthia_guardian::SELF_REFLECT_TOOL_NAME
                            });
                        if llm_self_reflect {
                            ctx.record_self_reflect_call();
                            last_reflect_iteration = Some(ctx.iteration);
                        }

                        // Detect an LLM-driven `compact_context` call so the
                        // post-tool-execution block can run the actual
                        // compaction and emit matching analytics. The
                        // tool's `call()` is a facade (P3 lazy loading) —
                        // the real mutation happens later in the loop so
                        // it does not race with the prefix snapshot above.
                        let llm_compact_context = sampling
                            .tool_calls
                            .iter()
                            .any(|c| c.name == COMPACT_CONTEXT_TOOL_NAME);

                        // Verify post-call prefix stability, then emit the
                        // event. Re-compute messages prefix (may have
                        // changed during the LLM call); system and tools
                        // are reused from the pre-call snapshot since
                        // neither should change during a call.
                        let post_messages_bytes =
                            PrefixTracker::canonical_messages_prefix_bytes(&ctx.messages);
                        let _stable = prefix_tracker.lock().record_post(
                            &system_snapshot,
                            &tools_schema_bytes,
                            &post_messages_bytes,
                            ctx.iteration as u64,
                        );
                        let event = prefix_tracker
                            .lock()
                            .emit_stability_event(ctx.iteration as u64);
                        if let Some(ref cb) = on_prefix_event {
                            cb(event);
                        }

                        // Fire after_llm hooks via UnifiedHookDispatcher
                        let after_llm_event = synthia_hook::HookEvent::PostResponse(
                            synthia_hook::outcome::PostResponsePayload {
                                session_id: ctx.session_id.clone(),
                                response_summary: sampling.text.chars().take(200).collect(),
                            },
                        );
                        let after_llm_outcome = steps.hook_dispatcher.dispatch(&after_llm_event).await;
                        handle_hook_outcome(
                            &after_llm_outcome,
                            &steps.steering_channel,
                            &mut ctx.forwarded_this_turn,
                        ).await;

                        // Accumulate token usage
                        ctx.cumulative_tokens += sampling.usage.total_tokens;

                        // Record token usage in rollout tracker
                        if let Some(ref rollout) = rollout_tracker {
                            rollout
                                .record_token_usage(
                                    sampling.usage.prompt_tokens,
                                    sampling.usage.completion_tokens,
                                    sampling.usage.cached_prompt_tokens
                                        .unwrap_or(0),
                                )
                                .await;
                        }

                        // Emit usage callback for OTel cache token metrics
                        // (KV cache hit ratio observability).
                        if let Some(ref cb) = on_usage {
                            cb(sampling.usage.clone());
                        }

                        if sampling.tool_calls.is_empty() {
                            ctx.set_end_reason(crate::events::SessionEndReason::Completed);
                            // Persist the assistant's text reply into the
                            // conversation log so any continuation of the
                            // session (e.g. auto-triggered self_reflect
                            // or compaction re-prompting) sees the
                            // coherent user/assistant pair. Without this,
                            // a subsequent LLM call would see the user's
                            // question but no assistant reply.
                            ctx.add_assistant_message_from_sampling(&sampling);
                            yield AgentEvent::ModelDone(sampling.clone());
                            current_turn.transition_to(TurnStatus::Completed);
                            emit_turn_event(
                                session_store.event_store(),
                                &session_store,
                                &user_id,
                                &session_id_clone,
                                TURN_COMPLETED,
                                turn_id,
                                ctx.iteration,
                                serde_json::json!({"outcome": "text_only"}),
                            )
                            .await;

                            for ev in super::super::iteration::maybe_auto_trigger_self_reflect(
                                &steps.tool_execute,
                                &mut ctx,
                                &mut last_reflect_iteration,
                            )
                            .await
                            {
                                yield ev;
                            }

                            // Text-only response: no tool calls, so the LLM
                            // did not invoke `compact_context` this iteration.
                            // Auto-trigger is still possible when the token
                            // ratio exceeds 80%.
                            for ev in super::super::iteration::maybe_auto_trigger_compact_context(
                                &steps.compact,
                                &mut ctx,
                                &config,
                                &mut last_compact_iteration,
                                false,
                                &steps.hook_dispatcher,
                            )
                            .await
                            {
                                yield ev;
                            }

                            break;
                        }

                        if let Some(loop_reason) = super::super::iteration::check_doom_loop(
                            &mut loop_detectors,
                            &sampling,
                            &mut ctx,
                        ) {
                            yield AgentEvent::System(SystemEvent::Warning {
                                kind: WarningKind::Loop,
                                message: loop_reason.clone(),
                                iteration: Some(ctx.iteration),
                            });
                            current_turn.fail_with("doom_loop_detected");
                            #[cfg(feature = "otel")]
                            turn_span_guard.record_error("TurnError", "doom_loop_detected");
                            emit_turn_event(
                                session_store.event_store(),
                                &session_store,
                                &user_id,
                                &session_id_clone,
                                TURN_FAILED,
                                turn_id,
                                ctx.iteration,
                                serde_json::json!({"reason": loop_reason}),
                            )
                            .await;
                            emit_turn_event(
                                session_store.event_store(),
                                &session_store,
                                &user_id,
                                &session_id_clone,
                                SESSION_ENDED,
                                turn_id,
                                ctx.iteration,
                                serde_json::json!({"reason": "LoopDetected"}),
                            )
                            .await;
                            yield AgentEvent::System(SystemEvent::SessionEnded {
                                reason: crate::events::SessionEndReason::LoopDetected,
                            });
                            return;
                        }

                        yield AgentEvent::ModelDone(sampling.clone());

                        // Persist the assistant's tool-use declaration
                        // into the conversation log BEFORE running any
                        // tool. The OpenAI / Anthropic contract requires
                        // every `Role::Tool` message to be preceded by
                        // an assistant message declaring the matching
                        // `tool_call_id`; without this the next request
                        // is rejected with
                        // `tool result's tool id not found`. See the
                        // 2026-07-25 server log at 10:58:30.080120.
                        ctx.add_assistant_message_from_sampling(&sampling);

                        current_turn.transition_to(TurnStatus::Executing);
                        emit_turn_event(
                            session_store.event_store(),
                            &session_store,
                            &user_id,
                            &session_id_clone,
                            TOOL_CALL_ISSUED,
                            turn_id,
                            ctx.iteration,
                            serde_json::json!({
                                "tool_call_count": sampling.tool_calls.len(),
                            }),
                        )
                        .await;

                        // ── InterceptorChain BeforeTool dispatch ──
                        // When an interceptor chain is configured, dispatch
                        // BeforeTool events for each tool call. If any call
                        // is short-circuited (e.g. by PermissionInterceptor),
                        // skip that tool and emit a ToolCallSkipped event.
                        let mut skipped_tools: Vec<String> = Vec::new();
                        if let Some(ref chain) = interceptor_chain {
                            for tool_call in &sampling.tool_calls {
                                let mut ic = crate::interceptor::InterceptorContext::new(
                                    &session_id_clone,
                                    &session_id_clone,
                                );
                                ic.iteration = ctx.iteration;
                                // Pass tool arguments as JSON string for
                                // LoopDetectInterceptor.
                                if let Ok(args_str) = serde_json::to_string(&tool_call.input) {
                                    let map = ic.ensure_data_object();
                                    map.insert(
                                        "tool_args".to_string(),
                                        serde_json::Value::String(args_str),
                                    );
                                }
                                let event =
                                    crate::interceptor::InterceptorEvent::BeforeTool {
                                        tool_name: tool_call.name.clone(),
                                    };
                                match chain.dispatch(&mut ic, &event).await {
                                    Ok(()) => {}
                                    Err(
                                        crate::interceptor::InterceptorError::ShortCircuited { name },
                                    ) => {
                                        tracing::warn!(
                                            tool = %tool_call.name,
                                            interceptor = %name,
                                            "InterceptorChain short-circuited tool call"
                                        );
                                        skipped_tools.push(tool_call.name.clone());
                                        yield AgentEvent::Model(synthia_provider::ContentPart::ToolResult(
                                            synthia_provider::ToolResult {
                                                tool_use_id: tool_call.id.clone(),
                                                content: vec![synthia_provider::ContentPart::Text(
                                                    synthia_provider::TextContent {
                                                        text: format!(
                                                            "Tool call blocked by interceptor: {name}"
                                                        ),
                                                        cache_control: None,
                                                    },
                                                )],
                                                structured_content: None,
                                                is_error: Some(true),
                                            },
                                        ));
                                        ctx.add_tool_result(
                                            tool_call.name.clone(),
                                            tool_call.id.clone(),
                                            format!(
                                                "Tool call blocked by interceptor: {name}"
                                            ),
                                            false,
                                        );
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            tool = %tool_call.name,
                                            error = %e,
                                            "InterceptorChain dispatch failed, proceeding with execution"
                                        );
                                    }
                                }
                            }
                        }

                        let tool_outcome = super::super::tool_execution::execute_and_emit(
                            &mut steps,
                            &mut ctx,
                            &sampling,
                            &config,
                            &session_id_clone,
                            cancel_token.clone(),
                            &mut loop_detectors,
                            compaction_provider_runtime.as_deref(),
                            memory_event_sender.as_ref(),
                            &agent_ctx,
                            loop_services.get().and_then(|s| s.output_bound.as_ref()),
                        )
                        .await;
                        match tool_outcome {
                            super::super::tool_execution::ToolExecuteOutcome::Continue { events } => {
                                for ev in events { yield ev; }

                                // Record file changes in rollout tracker for
                                // tools that modify files. Only record for
                                // tools that were not skipped by interceptors.
                                if let Some(ref rollout) = rollout_tracker {
                                    for tool_call in &sampling.tool_calls {
                                        if skipped_tools.contains(&tool_call.name) {
                                            continue;
                                        }
                                        let (change_type, file_path) =
                                            match tool_call.name.as_str() {
                                                "write" => {
                                                    let p = tool_call
                                                        .input
                                                        .get("file_path")
                                                        .or_else(|| {
                                                            tool_call.input.get("path")
                                                        })
                                                        .and_then(|v| v.as_str())
                                                        .unwrap_or("");
                                                    (ChangeType::Created, p)
                                                }
                                                "edit" | "multi_edit" => {
                                                    let p = tool_call
                                                        .input
                                                        .get("file_path")
                                                        .or_else(|| {
                                                            tool_call.input.get("path")
                                                        })
                                                        .and_then(|v| v.as_str())
                                                        .unwrap_or("");
                                                    (ChangeType::Modified, p)
                                                }
                                                "apply_patch" => {
                                                    let p = tool_call
                                                        .input
                                                        .get("path")
                                                        .or_else(|| {
                                                            tool_call
                                                                .input
                                                                .get("file_path")
                                                        })
                                                        .and_then(|v| v.as_str())
                                                        .unwrap_or("");
                                                    (ChangeType::Modified, p)
                                                }
                                                _ => continue,
                                            };
                                        if !file_path.is_empty() {
                                            rollout
                                                .record_change(FileChange {
                                                    path: std::path::PathBuf::from(
                                                        file_path,
                                                    ),
                                                    change_type,
                                                    tool_name: tool_call.name.clone(),
                                                    iteration: ctx.iteration,
                                                })
                                                .await;
                                        }
                                    }
                                }

                                // ── InterceptorChain AfterTool dispatch ──
                                // When an interceptor chain is configured,
                                // dispatch AfterTool events for each tool
                                // call that was not skipped by BeforeTool.
                                if let Some(ref chain) = interceptor_chain {
                                    for tool_call in &sampling.tool_calls {
                                        if skipped_tools.contains(&tool_call.name) {
                                            continue;
                                        }
                                        let mut ic = crate::interceptor::InterceptorContext::new(
                                            &session_id_clone,
                                            &session_id_clone,
                                        );
                                        ic.iteration = ctx.iteration;
                                        // Write cumulative token usage so
                                        // CompactInterceptor can check the
                                        // threshold.
                                        let map = ic.ensure_data_object();
                                        map.insert(
                                            "token_usage".to_string(),
                                            serde_json::Value::Number(
                                                serde_json::Number::from(
                                                    ctx.cumulative_tokens,
                                                ),
                                            ),
                                        );
                                        let event =
                                            crate::interceptor::InterceptorEvent::AfterTool {
                                                tool_name: tool_call.name.clone(),
                                            };
                                        if let Err(e) = chain.dispatch(&mut ic, &event).await {
                                            tracing::debug!(
                                                tool = %tool_call.name,
                                                error = %e,
                                                "InterceptorChain AfterTool dispatch error (non-blocking)"
                                            );
                                        }
                                    }
                                }

                                for ev in super::super::iteration::maybe_auto_trigger_self_reflect(
                                    &steps.tool_execute,
                                    &mut ctx,
                                    &mut last_reflect_iteration,
                                )
                                .await
                                {
                                    yield ev;
                                }

                                // Auto-trigger check runs BEFORE the
                                // LLM-driven compaction so the
                                // `llm_compact_context` dedup flag is
                                // exercised. When the LLM already requested
                                // compaction this iteration, the auto-trigger
                                // is skipped to avoid double compaction.
                                for ev in super::super::iteration::maybe_auto_trigger_compact_context(
                                    &steps.compact,
                                    &mut ctx,
                                    &config,
                                    &mut last_compact_iteration,
                                    llm_compact_context,
                                    &steps.hook_dispatcher,
                                )
                                .await
                                {
                                    yield ev;
                                }

                                // LLM-driven compaction: the facade tool
                                // already acknowledged the request; run the
                                // real compaction here and emit the analytics
                                // with `trigger = ToolCall`. The auto-trigger
                                // above was skipped (dedup flag), so at most
                                // one `ContextCompacted` is emitted per
                                // iteration.
                                if llm_compact_context
                                    && let Some(result) =
                                        steps.compact.execute(&mut ctx, &config)
                                {
                                    // Dispatch PreCompact/PostCompact for
                                    // LLM-driven compaction.
                                    let pre_compact = synthia_hook::HookEvent::PreCompact(
                                        synthia_hook::outcome::PreCompactPayload {
                                            session_id: ctx.session_id.clone(),
                                            token_count: result.old_tokens,
                                        },
                                    );
                                    steps.hook_dispatcher.dispatch(&pre_compact).await;

                                    last_compact_iteration =
                                        Some(ctx.iteration);
                                    CompactionAnalyticsAttempt::new(
                                        result.old_tokens,
                                        CompactionTrigger::ToolCall,
                                        "llm-tool-call",
                                        result.implementation.clone(),
                                        result.phase.clone(),
                                    )
                                    .emit();
                                    yield AgentEvent::System(SystemEvent::Warning {
                                        kind: WarningKind::ContextCompaction,
                                        message: format!(
                                            "Compacted {} -> {} tokens",
                                            result.old_tokens, result.new_tokens
                                        ),
                                        iteration: Some(ctx.iteration),
                                    });

                                    let post_compact = synthia_hook::HookEvent::PostCompact(
                                        synthia_hook::outcome::PostCompactPayload {
                                            session_id: ctx.session_id.clone(),
                                            token_count: result.new_tokens,
                                        },
                                    );
                                    steps.hook_dispatcher.dispatch(&post_compact).await;
                                }
                            }
                            super::super::tool_execution::ToolExecuteOutcome::Terminate { events } => {
                                for ev in events { yield ev; }
                                current_turn.fail_with("tool_execution_terminate");
                                #[cfg(feature = "otel")]
                                turn_span_guard.record_error("TurnError", "tool_execution_terminate");
                                emit_turn_event(
                                    session_store.event_store(),
                                    &session_store,
                                    &user_id,
                                    &session_id_clone,
                                    TURN_FAILED,
                                    turn_id,
                                    ctx.iteration,
                                    serde_json::json!({"reason": "tool_execution_terminate"}),
                                )
                                .await;
                                emit_turn_event(
                                    session_store.event_store(),
                                    &session_store,
                                    &user_id,
                                    &session_id_clone,
                                    SESSION_ENDED,
                                    turn_id,
                                    ctx.iteration,
                                    serde_json::json!({"reason": "ToolExecutionTerminate"}),
                                )
                                .await;
                                return;
                            }
                        }

                        emit_turn_event(
                            session_store.event_store(),
                            &session_store,
                            &user_id,
                            &session_id_clone,
                            TOOL_RESULT_RECEIVED,
                            turn_id,
                            ctx.iteration,
                            serde_json::Value::Null,
                        )
                        .await;
                        current_turn.transition_to(TurnStatus::Completed);
                        emit_turn_event(
                            session_store.event_store(),
                            &session_store,
                            &user_id,
                            &session_id_clone,
                            TURN_COMPLETED,
                            turn_id,
                            ctx.iteration,
                            serde_json::Value::Null,
                        )
                        .await;

                        // IterationCompleted is no longer emitted on the
                        // wire (Phase 2 of simplify-agent-event-stream);
                        // internal iteration tracking is preserved via
                        // ctx.iteration.

                        // ── Goal tracking ──
                        // Update the goal tracker with iteration progress.
                        if let Some(services) = loop_services.get()
                            && let Some(ref tracker) = services.goal_tracker
                        {
                            let mut budget = tracker.budget().await;
                            budget.iterations_used += 1;
                            budget.tokens_used =
                                ctx.cumulative_tokens as u64;
                            let current = tracker.current().await;
                            if let Some(mut goal) = current {
                                goal.budget = budget;
                                tracker.set(goal).await;
                            }
                        }
                    }
                }
            }

            // Loop exit: if no explicit end_reason was set inside the
            // while loop and we hit the iteration cap, surface that as
            // the reason. Without this fallback, a session that ends
            // solely because `iteration >= max_iterations` would
            // silently fall through to `Completed`.
            if ctx.end_reason.is_none()
                && ctx.iteration >= config.max_iterations
            {
                ctx.set_end_reason(crate::events::SessionEndReason::MaxIterationsReached);
            }

            let end_reason = ctx.end_reason.clone().unwrap_or(crate::events::SessionEndReason::Completed);

            // Dispatch SessionEnd hook event via UnifiedHookDispatcher
            let session_end_reason = match &end_reason {
                crate::events::SessionEndReason::Completed => synthia_hook::outcome::SessionEndReason::Completed,
                crate::events::SessionEndReason::Cancelled => synthia_hook::outcome::SessionEndReason::Cancelled,
                crate::events::SessionEndReason::Error(_) => synthia_hook::outcome::SessionEndReason::Error,
                crate::events::SessionEndReason::TokenBudgetExceeded => synthia_hook::outcome::SessionEndReason::Error,
                crate::events::SessionEndReason::MaxIterationsReached => synthia_hook::outcome::SessionEndReason::Error,
                crate::events::SessionEndReason::GuardianBlocked => synthia_hook::outcome::SessionEndReason::Error,
                crate::events::SessionEndReason::LoopDetected => synthia_hook::outcome::SessionEndReason::Error,
                crate::events::SessionEndReason::CircuitBreakerOpen => synthia_hook::outcome::SessionEndReason::Error,
            };
            let session_end_event = synthia_hook::HookEvent::SessionEnd(
                synthia_hook::outcome::SessionEndPayload {
                    session_id: session_id_clone.clone(),
                    reason: session_end_reason,
                },
            );
            steps.hook_dispatcher.dispatch(&session_end_event).await;

            // ── InterceptorChain SessionEnd dispatch ──
            // Notify interceptors that the session is ending so they can
            // reset their state (e.g. LoopDetectInterceptor resets its
            // detector on SessionEnd).
            if let Some(ref chain) = interceptor_chain {
                let mut ic = crate::interceptor::InterceptorContext::new(
                    &session_id_clone,
                    &session_id_clone,
                );
                ic.iteration = ctx.iteration;
                if let Err(e) = chain.dispatch(&mut ic, &crate::interceptor::InterceptorEvent::SessionEnd).await {
                    tracing::debug!(
                        error = %e,
                        "InterceptorChain SessionEnd dispatch error (non-blocking)"
                    );
                }
            }

            let final_turn_id = ctx.current_turn_id.unwrap_or_default();
            emit_turn_event(
                session_store.event_store(),
                &session_store,
                &user_id,
                &session_id_clone,
                SESSION_ENDED,
                final_turn_id,
                ctx.iteration,
                serde_json::json!({"reason": end_reason}),
            )
            .await;

            super::super::iteration::end_of_session_reflect(
                &steps.reflect,
                provider.clone(),
                &ctx,
                last_reflect_iteration,
                memory_event_sender.as_ref(),
                &session_id_clone,
            )
            .await;

            yield AgentEvent::System(SystemEvent::SessionEnded { reason: end_reason });
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use synthia_core::tool::{
        builtin_fragments::SystemPromptFragment,
        extension_registry::ExtensionRegistry,
        fragment::FragmentRegistry,
        registry::ToolRegistry as CoreToolRegistry,
        rollout::{ChangeType, FileChange, RolloutTracker},
    };

    use super::*;
    use crate::{
        control::CompletedTask,
        interceptor::InterceptorChain,
        registry::instance::AgentStatus,
    };

    #[test]
    fn format_completed_task_notification() {
        let task = CompletedTask {
            agent_id: "bg-42".to_string(),
            output: "result data".to_string(),
            status: AgentStatus::Completed,
        };
        let xml = format_background_task_notification(&task);
        assert!(xml.contains("<task id=\"bg-42\" state=\"completed\">"));
        assert!(
            xml.contains("<summary>Background task completed: bg-42</summary>")
        );
        assert!(xml.contains("<task_result>"));
        assert!(xml.contains("result data"));
        assert!(xml.contains("</task_result>"));
        assert!(xml.contains("</task>"));
    }

    #[test]
    fn format_errored_task_notification() {
        let task = CompletedTask {
            agent_id: "bg-err".to_string(),
            output: "something went wrong".to_string(),
            status: AgentStatus::Errored,
        };
        let xml = format_background_task_notification(&task);
        assert!(xml.contains("<task id=\"bg-err\" state=\"error\">"));
        assert!(
            xml.contains("<summary>Background task error: bg-err</summary>")
        );
        assert!(xml.contains("<task_error>"));
        assert!(xml.contains("something went wrong"));
        assert!(xml.contains("</task_error>"));
    }

    #[test]
    fn format_notification_escapes_task_output() {
        let task = CompletedTask {
            agent_id: "bg-x".to_string(),
            output: "<raw>value</raw>".to_string(),
            status: AgentStatus::Completed,
        };
        let xml = format_background_task_notification(&task);
        assert!(xml.contains("<raw>value</raw>"));
    }

    // ── FragmentRegistry activation test ─────────────────────────────────
    //
    // Mirrors the main_loop's FragmentRegistry integration:
    // 1. Build ExtensionRegistry with registered fragments
    // 2. Call render_active() to get rendered fragments
    // 3. Construct system content using the same format as main_loop
    // 4. Inject into messages (replace existing system msg or prepend)
    #[tokio::test]
    async fn fragment_registry_activation_builds_system_prompt() {
        // Build FragmentRegistry with a SystemPromptFragment
        let frag_reg = Arc::new(FragmentRegistry::new());
        frag_reg
            .register(Arc::new(SystemPromptFragment::new(
                "You are Synthia, an AI coding assistant.",
            )))
            .await
            .unwrap();

        // Build ExtensionRegistry
        let ext_reg =
            ExtensionRegistry::new(Arc::new(CoreToolRegistry::new()), frag_reg);

        // Simulate main_loop's FragmentRegistry integration
        let frag_reg = ext_reg.fragment_registry();
        let frag_ctx = FragmentContext::new("test-session", "test-user");
        let rendered = frag_reg.render_active(&frag_ctx).await;

        // At least one fragment rendered
        assert!(
            !rendered.is_empty(),
            "expected at least one rendered fragment"
        );

        // Build system content using the same format as main_loop
        let system_content: String = rendered
            .iter()
            .map(|(name, content)| format!("## {name}\n{content}"))
            .collect::<Vec<String>>()
            .join("\n\n");

        assert!(
            system_content.contains("system_prompt"),
            "system prompt fragment should be rendered"
        );
        assert!(
            system_content.contains("You are Synthia"),
            "system prompt content should be included"
        );

        // Inject into messages — same logic as main_loop
        let mut messages: Vec<Message> = vec![Message::system("old system")];
        if let Some(first) = messages.first_mut()
            && first.role == synthia_provider::Role::System
        {
            first.content =
                synthia_provider::types::Content::text(system_content.clone());
        }
        assert_eq!(messages.len(), 1);
        let extracted = messages[0].content.extract_text();
        assert_eq!(extracted, Some(system_content));
    }

    // ── FragmentRegistry: inactive fragments are excluded ────────────────
    #[tokio::test]
    async fn fragment_registry_inactive_fragments_excluded() {
        let frag_reg = Arc::new(FragmentRegistry::new());
        // Register an inactive fragment
        frag_reg
            .register(Arc::new(
                SystemPromptFragment::new("inactive content")
                    .with_active(false),
            ))
            .await
            .unwrap();

        let ext_reg =
            ExtensionRegistry::new(Arc::new(CoreToolRegistry::new()), frag_reg);

        let frag_ctx = FragmentContext::new("test-session", "test-user");
        let rendered =
            ext_reg.fragment_registry().render_active(&frag_ctx).await;

        assert!(
            rendered.is_empty(),
            "inactive fragments should not be rendered"
        );
    }

    // ── InterceptorChain dispatch test ──────────────────────────────────
    //
    // Mirrors the main_loop's InterceptorChain integration:
    // 1. Build InterceptorChain with TraceInterceptor
    // 2. Dispatch BeforeTool/AfterTool events
    // 3. Verify dispatch succeeds (no short-circuit)
    // 4. Dispatch SessionEnd event
    #[tokio::test]
    async fn interceptor_chain_before_after_tool_dispatch() {
        use crate::interceptor::{InterceptorContext, InterceptorEvent};

        let chain = InterceptorChain::default_with_guard(None, None, None);
        // The default chain includes TraceInterceptor; dispatch should succeed.
        let session_id = "test-session";

        // BeforeTool
        let mut ic = InterceptorContext::new(session_id, session_id);
        ic.iteration = 1;
        let event = InterceptorEvent::BeforeTool {
            tool_name: "bash".to_string(),
        };
        let result = chain.dispatch(&mut ic, &event).await;
        assert!(result.is_ok(), "BeforeTool dispatch should succeed");

        // AfterTool
        let mut ic = InterceptorContext::new(session_id, session_id);
        ic.iteration = 1;
        let map = ic.ensure_data_object();
        map.insert(
            "token_usage".to_string(),
            serde_json::Value::Number(serde_json::Number::from(1000)),
        );
        let event = InterceptorEvent::AfterTool {
            tool_name: "bash".to_string(),
        };
        let result = chain.dispatch(&mut ic, &event).await;
        assert!(result.is_ok(), "AfterTool dispatch should succeed");
    }

    // ── InterceptorChain: short-circuit behavior ────────────────────────
    #[tokio::test]
    async fn interceptor_chain_short_circuit_blocks_tool() {
        use crate::interceptor::{
            InterceptorContext,
            InterceptorEvent,
            PermissionInterceptor,
        };

        // Add a blocking permission interceptor for "bash"
        let mut custom_chain = InterceptorChain::new();
        custom_chain.add(Arc::new(PermissionInterceptor::blocking(&["bash"])));

        let mut ic = InterceptorContext::new("test-session", "test-session");
        ic.iteration = 1;
        let event = InterceptorEvent::BeforeTool {
            tool_name: "bash".to_string(),
        };
        let result = custom_chain.dispatch(&mut ic, &event).await;
        assert!(result.is_err(), "blocked tool should short-circuit");
        match result.unwrap_err() {
            crate::interceptor::InterceptorError::ShortCircuited { name } => {
                assert!(
                    name.contains("permission"),
                    "short-circuit should be from PermissionInterceptor, got: {name}"
                );
            }
            other => panic!("expected ShortCircuited, got {other}"),
        }

        // Non-blocked tool should pass
        let mut ic = InterceptorContext::new("test-session", "test-session");
        let event = InterceptorEvent::BeforeTool {
            tool_name: "read_file".to_string(),
        };
        let result = custom_chain.dispatch(&mut ic, &event).await;
        assert!(result.is_ok(), "non-blocked tool should pass");
    }

    // ── InterceptorChain: SessionEnd dispatch ───────────────────────────
    #[tokio::test]
    async fn interceptor_chain_session_end_dispatch() {
        use crate::interceptor::{InterceptorContext, InterceptorEvent};

        let chain = InterceptorChain::default_with_guard(None, None, None);
        let mut ic = InterceptorContext::new("test-session", "test-session");
        ic.iteration = 5;
        let event = InterceptorEvent::SessionEnd;
        let result = chain.dispatch(&mut ic, &event).await;
        assert!(result.is_ok(), "SessionEnd dispatch should succeed");
    }

    // ── RolloutTracker call test ────────────────────────────────────────
    //
    // Mirrors the main_loop's RolloutTracker integration:
    // 1. Record token usage after LLM response
    // 2. Record file changes after tool execution
    // 3. Verify summary reflects all recorded data
    #[tokio::test]
    async fn rollout_tracker_records_token_and_file_changes() {
        let tracker = RolloutTracker::new();

        // Simulate LLM response — record token usage
        tracker.record_token_usage(500, 200, 50).await;
        tracker.record_token_usage(300, 150, 30).await;

        // Simulate tool execution — record file changes
        tracker
            .record_change(FileChange {
                path: std::path::PathBuf::from("src/main.rs"),
                change_type: ChangeType::Modified,
                tool_name: "edit_file".to_string(),
                iteration: 1,
            })
            .await;
        tracker
            .record_change(FileChange {
                path: std::path::PathBuf::from("src/lib.rs"),
                change_type: ChangeType::Created,
                tool_name: "write_file".to_string(),
                iteration: 2,
            })
            .await;
        tracker
            .record_change(FileChange {
                path: std::path::PathBuf::from("src/old.rs"),
                change_type: ChangeType::Deleted,
                tool_name: "bash".to_string(),
                iteration: 3,
            })
            .await;

        // Verify token budget
        let budget = tracker.token_budget().await;
        assert_eq!(budget.prompt_tokens, 800);
        assert_eq!(budget.completion_tokens, 350);
        assert_eq!(budget.cached_tokens, 80);
        assert_eq!(budget.total_used, 1150);

        // Verify changes
        let changes = tracker.changes().await;
        assert_eq!(changes.len(), 3);

        // Verify summary
        let summary = tracker.summary().await;
        assert_eq!(summary.total_changes, 3);
        assert_eq!(summary.files_created, 1);
        assert_eq!(summary.files_modified, 1);
        assert_eq!(summary.files_deleted, 1);
        assert_eq!(summary.total_tokens_used, 1150);

        // Verify by-tool grouping (mirrors main_loop's tool name tracking)
        let by_tool = tracker.changes_by_tool().await;
        assert_eq!(by_tool.get("edit_file"), Some(&1));
        assert_eq!(by_tool.get("write_file"), Some(&1));
        assert_eq!(by_tool.get("bash"), Some(&1));
    }

    // ── RolloutTracker: with token limit ────────────────────────────────
    #[tokio::test]
    async fn rollout_tracker_with_token_limit() {
        let tracker = RolloutTracker::new_with_token_limit(1000);

        tracker.record_token_usage(600, 300, 50).await;
        let budget = tracker.token_budget().await;
        assert_eq!(budget.total_used, 900);
        assert!(!budget.is_exhausted());

        tracker.record_token_usage(100, 50, 0).await;
        let budget = tracker.token_budget().await;
        assert!(budget.is_exhausted());

        let summary = tracker.summary().await;
        assert_eq!(summary.token_remaining, Some(0));
    }

    // ── Full integration: ExtensionRegistry + InterceptorChain + RolloutTracker ──
    //
    // Simulates the full wiring path used in main_loop:
    // AppState → AgentFactory → AgentRunConfig → main_loop
    #[tokio::test]
    async fn full_wiring_integration() {
        // Build FragmentRegistry
        let frag_reg = Arc::new(FragmentRegistry::new());
        frag_reg
            .register(Arc::new(SystemPromptFragment::new(
                "You are Synthia, an AI coding assistant.",
            )))
            .await
            .unwrap();

        // Build ExtensionRegistry
        let ext_reg =
            ExtensionRegistry::new(Arc::new(CoreToolRegistry::new()), frag_reg);

        // Build InterceptorChain
        let chain = InterceptorChain::default_with_guard(None, None, None);

        // Build RolloutTracker
        let rollout = RolloutTracker::new();

        // Simulate FragmentRegistry → system prompt
        let frag_ctx = FragmentContext::new("test-session", "test-user");
        let rendered =
            ext_reg.fragment_registry().render_active(&frag_ctx).await;
        assert!(!rendered.is_empty());
        let system_content: String = rendered
            .iter()
            .map(|(name, content)| format!("## {name}\n{content}"))
            .collect::<Vec<String>>()
            .join("\n\n");
        assert!(system_content.contains("Synthia"));

        // Simulate InterceptorChain → BeforeTool dispatch
        use crate::interceptor::{InterceptorContext, InterceptorEvent};
        let mut ic = InterceptorContext::new("test-session", "test-session");
        ic.iteration = 1;
        let event = InterceptorEvent::BeforeTool {
            tool_name: "read_file".to_string(),
        };
        assert!(chain.dispatch(&mut ic, &event).await.is_ok());

        // Simulate RolloutTracker → record token + file change
        rollout.record_token_usage(500, 200, 50).await;
        rollout
            .record_change(FileChange {
                path: std::path::PathBuf::from("src/main.rs"),
                change_type: ChangeType::Modified,
                tool_name: "edit_file".to_string(),
                iteration: 1,
            })
            .await;

        let summary = rollout.summary().await;
        assert_eq!(summary.total_changes, 1);
        assert_eq!(summary.total_tokens_used, 700);

        // Simulate SessionEnd
        let event = InterceptorEvent::SessionEnd;
        assert!(chain.dispatch(&mut ic, &event).await.is_ok());
    }
}
