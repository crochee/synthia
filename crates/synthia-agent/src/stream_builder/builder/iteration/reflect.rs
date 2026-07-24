//! Self-reflection helpers for the per-iteration loop.

use std::sync::Arc;

use synthia_context::truncate::{TruncateConfig, truncate_output};
use synthia_memory::types::MemoryEvent;
use synthia_provider::traits::ModelProvider;
use tokio::sync::mpsc::Sender;
use tracing::warn;

use crate::{
    events::AgentEvent,
    loop_context::LoopContext,
    stream_builder::steps::{StepReflect, StepToolExecute},
    types::ToolResult,
};

/// Execute a synthetic `self_reflect` tool call through the same
/// registry/orchestrator path used for LLM-driven tool calls.
///
/// Returns the [`ToolResult`] and the emitted `ToolCallStarted` /
/// `ToolCallCompleted` events so the caller can yield them and record
/// the result in the loop context.
pub(crate) async fn execute_self_reflect_tool_call(
    step: &StepToolExecute,
    ctx: &mut LoopContext,
) -> Result<(ToolResult, Vec<AgentEvent>), synthia_core::Error> {
    use synthia_provider::types::ToolUse;

    let tool_use = ToolUse {
        id: format!("self_reflect-auto-{}-{}", ctx.session_id, ctx.iteration),
        name: synthia_guardian::SELF_REFLECT_TOOL_NAME.to_string(),
        input: serde_json::json!({}),
    };

    let mut events = vec![AgentEvent::ToolCallStarted {
        tool_name: tool_use.name.clone(),
        input: tool_use.input.clone(),
    }];

    let mut results = step.execute(ctx, vec![tool_use.clone()]).await?;
    let result = results.pop().ok_or_else(|| {
        synthia_core::Error::Internal(
            "self_reflect auto-trigger returned no result".to_string(),
        )
    })?;

    let truncate_cfg = TruncateConfig {
        session_id: Some(ctx.session_id.clone()),
        tool_call_id: Some(result.tool_call_id.clone()),
        ..TruncateConfig::default()
    };
    let truncate_result = truncate_output(&result.output, &truncate_cfg);
    let effective_output = if truncate_result.truncated {
        truncate_result.output
    } else {
        result.output.clone()
    };

    events.push(AgentEvent::ToolCallCompleted {
        tool_name: result.tool_name.clone(),
        output: effective_output.clone(),
        is_error: result.is_error,
    });

    let result = ToolResult {
        tool_name: result.tool_name,
        tool_call_id: result.tool_call_id,
        output: effective_output,
        is_error: result.is_error,
    };

    Ok((result, events))
}

/// Run end-of-session self-reflection if appropriate.
///
/// The original condition was logic-inverted (fired on
/// almost every iteration). The corrected condition
/// fires EOS reflect only when:
/// 1. at least one iteration ran,
/// 2. the in-loop reflection did NOT fire on the final
///    iteration (no double-reflection), and
/// 3. the session actually exercised tool execution.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn end_of_session_reflect(
    step: &StepReflect,
    provider: Arc<dyn ModelProvider>,
    ctx: &LoopContext,
    last_reflect_iteration: Option<usize>,
    memory_event_sender: Option<&Sender<MemoryEvent>>,
    session_id: &str,
) {
    let needs_eos_reflect = ctx.iteration > 0
        && last_reflect_iteration != Some(ctx.iteration)
        && !ctx.recent_tool_results.is_empty();
    if !needs_eos_reflect {
        return;
    }
    match step.execute(provider, ctx).await {
        Ok(reflection) => {
            if let Some(sender) = memory_event_sender {
                let tools_used: Vec<String> = ctx
                    .recent_tool_results
                    .iter()
                    .map(|(name, _, _)| name.clone())
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect();
                if let Err(e) = sender
                    .send(MemoryEvent::session_end(
                        session_id.to_string(),
                        reflection.summary.clone(),
                        tools_used,
                        "completed".to_string(),
                    ))
                    .await
                {
                    warn!(error = %e, "Failed to send session_end memory event");
                }
            }
        }
        Err(e) => {
            warn!(error = %e, "Self-reflection failed");
        }
    }
}
