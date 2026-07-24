//! LLM sampling helpers for the per-iteration loop.

use std::sync::Arc;

use synthia_context::compact_context_tool::{
    COMPACT_CONTEXT_TOOL_NAME,
    compact_context_tool_definition,
};
use synthia_core::{Registry, RegistryItem};
use synthia_hook::AgentContext;
use synthia_provider::traits::ModelProvider;
use synthia_tool::registry::ToolRegistry;

use super::types::LlmSampleOutcome;
use crate::{
    config::AgentConfig,
    error_recovery::recovery_cascade::{RecoveryAction, run_recovery_cascade},
    events::{AgentEvent, SessionEndReason, SystemEvent},
    loop_context::LoopContext,
    stream_builder::builder::types::BuilderSteps,
};

/// Build the tool definitions for the LLM call from the
/// tool registry.
///
/// `current_tokens` is used to inject a dynamic
/// `<context_tokens>X</context_tokens>` hint into the
/// `compact_context` tool description, rounded to the
/// nearest 100 tokens so minor fluctuations do not
/// churn the tool schema / prompt cache key.
///
/// Returns an empty `Vec` if the registry list call
/// fails — the LLM call still goes through (just with
/// no tools), which matches the pre-refactor
/// behaviour.
pub(crate) async fn build_tool_definitions(
    tool_registry: &ToolRegistry,
    current_tokens: usize,
) -> Vec<synthia_provider::ToolDefinition> {
    tool_registry
        .list(None)
        .await
        .map(|entries| {
            entries
                .iter()
                .map(|e| {
                    if e.name() == COMPACT_CONTEXT_TOOL_NAME {
                        compact_context_tool_definition(current_tokens)
                    } else {
                        synthia_provider::ToolDefinition {
                            name: e.name().to_string(),
                            description: e.description().to_string(),
                            input_schema: e.tool_instance().parameters(),
                            cache_control: None,
                        }
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Prepare the [`AgentContext`] passed to the
/// before/after LLM hooks. Mirrors the
/// `AgentContext` shape that the original `builder.rs`
/// constructed inline.
pub(crate) fn prepare_agent_ctx(ctx: &LoopContext) -> AgentContext {
    let mut agent_ctx = AgentContext::new(
        ctx.session_id.clone(),
        ctx.current_turn_id
            .map(|t| t.0.to_string())
            .unwrap_or_else(|| format!("turn-{}", ctx.iteration)),
    );
    agent_ctx.iteration = ctx.iteration;
    agent_ctx.messages = ctx.messages.clone();
    agent_ctx
}

/// Sample from the LLM and run the LLM-side recovery
/// cascade on error.
///
/// On a successful sample, returns
/// `LlmSampleOutcome::Done` with one
/// `LlmStreamDelta` event per text delta. On
/// `RecoveryAction::Recovered`, returns
/// `LlmSampleOutcome::Continue` with a single
/// `RecoveryApplied` event. On
/// `RecoveryAction::FailFast` / `Escalate`, returns
/// `LlmSampleOutcome::Terminate` with
/// `LlmError` + `SessionEnded` events.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn sample_llm_and_cascade(
    steps: &mut BuilderSteps,
    provider: Arc<dyn ModelProvider>,
    ctx: &mut LoopContext,
    tool_definitions: Vec<synthia_provider::ToolDefinition>,
    cancel_token: tokio_util::sync::CancellationToken,
    config: &AgentConfig,
    loop_detectors: &mut synthia_guardian::LoopDetectorSet,
    compaction_provider: Option<
        &dyn synthia_context::compaction::level1::CompactionProvider,
    >,
) -> LlmSampleOutcome {
    let sample_result = steps
        .sample
        .execute(
            provider.clone(),
            ctx,
            tool_definitions,
            cancel_token.clone(),
        )
        .await;
    match sample_result {
        Ok((sampling, deltas)) => {
            steps.recovery.record_success();
            let events: Vec<AgentEvent> =
                deltas.into_iter().map(AgentEvent::Model).collect();
            LlmSampleOutcome::Done { sampling, events }
        }
        Err(e) => {
            tracing::error!(error = %e, "LLM sampling failed");
            // Validation errors are not transient — recovery actions
            // (L3 fallback, L4 compact, L5 reset) cannot turn a
            // permanent condition into a recoverable one. Running
            // them only re-enters `execute()` with the same bad
            // state, looping until the iteration cap. Detect the
            // validation case up front and terminate the session
            // with a clear error instead.
            if let synthia_core::Error::Validation(ref reason) = e {
                ctx.set_end_reason(SessionEndReason::Error(reason.clone()));
                return LlmSampleOutcome::Terminate {
                    events: vec![AgentEvent::System(
                        SystemEvent::SessionEnded {
                            reason: ctx.end_reason.clone().unwrap_or_else(
                                || {
                                    SessionEndReason::Error(
                                        "Unknown".to_string(),
                                    )
                                },
                            ),
                        },
                    )],
                };
            }
            let recovery_iteration = ctx.iteration;
            let steering_ref: Option<&dyn crate::steering::SteeringChannel> =
                steps.steering_channel.as_deref();
            let action = run_recovery_cascade(
                &e.to_string(),
                "llm_sample",
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
                    tracing::info!(
                        level,
                        "Recovery cascade recovered; continuing"
                    );
                    LlmSampleOutcome::Continue {
                        events: vec![AgentEvent::recovery(
                            level,
                            Some("llm_sample".to_string()),
                            message,
                            Some(recovery_iteration),
                        )],
                    }
                }
                RecoveryAction::FailFast(reason) => {
                    tracing::error!(
                        reason = %reason,
                        "Recovery cascade exhausted; entering fail-fast"
                    );
                    ctx.set_end_reason(SessionEndReason::Error(reason));
                    let end_reason =
                        ctx.end_reason.clone().unwrap_or_else(|| {
                            SessionEndReason::Error("Unknown".to_string())
                        });
                    LlmSampleOutcome::Terminate {
                        events: vec![AgentEvent::System(
                            SystemEvent::SessionEnded { reason: end_reason },
                        )],
                    }
                }
                RecoveryAction::Escalate => {
                    tracing::error!(
                        "Recovery cascade escalated (unexpected); entering fail-fast"
                    );
                    ctx.set_end_reason(SessionEndReason::Error(
                        "Recovery cascade escalated".to_string(),
                    ));
                    let end_reason =
                        ctx.end_reason.clone().unwrap_or_else(|| {
                            SessionEndReason::Error("Unknown".to_string())
                        });
                    LlmSampleOutcome::Terminate {
                        events: vec![AgentEvent::System(
                            SystemEvent::SessionEnded { reason: end_reason },
                        )],
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use synthia_tool::registry::{ToolEntry, ToolRegistry};

    use super::*;

    #[tokio::test]
    async fn build_tool_definitions_injects_token_hint_for_compact_context() {
        let reg = ToolRegistry::new();
        reg.register(ToolEntry::new(Arc::new(
            crate::tools::CompactContextTool,
        )));

        let defs = build_tool_definitions(&reg, 75_123).await;
        let compact = defs
            .iter()
            .find(|d| d.name == COMPACT_CONTEXT_TOOL_NAME)
            .expect("compact_context tool must be registered");
        // 75_123 rounds to 75_100 (nearest 100)
        assert!(
            compact
                .description
                .contains("<context_tokens>75100</context_tokens>"),
            "description was: {}",
            compact.description
        );
    }
}
