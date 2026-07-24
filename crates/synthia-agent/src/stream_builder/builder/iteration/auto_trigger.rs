//! Auto-trigger helpers for the per-iteration loop.
//!
//! These functions check whether an automatic action (self-reflection
//! or context compaction) should fire at the end of an iteration and
//! execute it if so.

use synthia_hook::UnifiedHookDispatcher;
use synthia_telemetry::{CompactionAnalyticsAttempt, CompactionTrigger};

use crate::{
    events::{AgentEvent, SystemEvent, WarningKind},
    loop_context::LoopContext,
    stream_builder::steps::{StepCompact, StepToolExecute},
    types::AgentConfig,
};

/// Auto-trigger the `self_reflect` tool if the current iteration has
/// reached the scheduled reflection point and the LLM did not already
/// invoke the tool.
///
/// The caller must have already called
/// [`LoopContext::record_self_reflect_call`] when the LLM requested
/// `self_reflect`; this helper therefore implicitly deduplicates within
/// the same iteration.
pub(crate) async fn maybe_auto_trigger_self_reflect(
    step: &StepToolExecute,
    ctx: &mut LoopContext,
    last_reflect_iteration: &mut Option<usize>,
) -> Vec<AgentEvent> {
    if ctx.iteration < ctx.next_self_reflect_iteration {
        return Vec::new();
    }

    match super::reflect::execute_self_reflect_tool_call(step, ctx).await {
        Ok((result, events)) => {
            ctx.add_tool_result(
                result.tool_name.clone(),
                result.tool_call_id,
                result.output,
                !result.is_error,
            );
            ctx.record_self_reflect_call();
            *last_reflect_iteration = Some(ctx.iteration);
            events
        }
        Err(e) => {
            vec![AgentEvent::System(SystemEvent::Warning {
                kind: WarningKind::Hook,
                message: format!("Auto self_reflect failed: {}", e),
                iteration: None,
            })]
        }
    }
}

/// Auto-trigger the `compact_context` compaction when the context
/// utilization exceeds 80% and the LLM did not already request it this
/// iteration.
///
/// Mirrors [`maybe_auto_trigger_self_reflect`] but for compaction. The
/// `llm_compact_called_this_iter` flag provides same-iteration dedup: when
/// the LLM already invoked `compact_context`, the auto-trigger is skipped so
/// the LLM-driven path (run by the caller immediately after this helper)
/// performs at most one compaction.
///
/// The 80% threshold is below the configured `TokenBudget`'s
/// `compaction_at` (85%) so the auto-trigger fires before the budget's own
/// `MustCompact` path takes over — giving the LLM a softer signal.
pub(crate) async fn maybe_auto_trigger_compact_context(
    compact: &StepCompact,
    ctx: &mut LoopContext,
    config: &AgentConfig,
    last_compact_iteration: &mut Option<usize>,
    llm_compact_called_this_iter: bool,
    hook_dispatcher: &UnifiedHookDispatcher,
) -> Vec<AgentEvent> {
    // Skip when the LLM already requested compaction this iteration.
    if llm_compact_called_this_iter {
        return Vec::new();
    }
    // Skip when context utilization is at or below 80%.
    if ctx.token_ratio() <= 0.8 {
        return Vec::new();
    }
    // Dispatch PreCompact hook event before auto-trigger compaction.
    let pre_compact = synthia_hook::HookEvent::PreCompact(
        synthia_hook::outcome::PreCompactPayload {
            session_id: ctx.session_id.clone(),
            token_count: ctx.cumulative_tokens,
        },
    );
    hook_dispatcher.dispatch(&pre_compact).await;

    match compact.execute(ctx, config) {
        Some(result) => {
            // Dispatch PostCompact hook event after auto-trigger compaction.
            let post_compact = synthia_hook::HookEvent::PostCompact(
                synthia_hook::outcome::PostCompactPayload {
                    session_id: ctx.session_id.clone(),
                    token_count: result.new_tokens,
                },
            );
            hook_dispatcher.dispatch(&post_compact).await;

            *last_compact_iteration = Some(ctx.iteration);
            CompactionAnalyticsAttempt::new(
                result.old_tokens,
                CompactionTrigger::AutoThreshold,
                "auto-threshold",
                result.implementation.clone(),
                result.phase.clone(),
            )
            .emit();
            vec![AgentEvent::System(SystemEvent::Warning {
                kind: WarningKind::ContextCompaction,
                message: format!(
                    "compacted {} -> {} tokens",
                    result.old_tokens, result.new_tokens
                ),
                iteration: Some(ctx.iteration),
            })]
        }
        None => Vec::new(),
    }
}
