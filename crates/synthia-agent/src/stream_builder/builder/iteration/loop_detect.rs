//! Doom-loop detection for the per-iteration loop.

use synthia_guardian::{LoopAction, LoopDetectorSet, LoopStatus};
use synthia_provider::types::SamplingResult;
use tracing::warn;

use crate::{events::SessionEndReason, loop_context::LoopContext};

/// Run the doom-loop detector over the tool calls
/// returned by the LLM.
///
/// Returns `Some(reason)` when a loop was detected so
/// the caller can yield `LoopWarning` and terminate.
pub(crate) fn check_doom_loop(
    loop_detectors: &mut LoopDetectorSet,
    sampling_result: &SamplingResult,
    ctx: &mut LoopContext,
) -> Option<String> {
    let mut doom_loop_tool: Option<String> = None;
    let mut loop_detected = false;
    for tu in &sampling_result.tool_calls {
        let input_json_str =
            serde_json::to_string(&tu.input).unwrap_or_default();
        let (status, action) =
            loop_detectors.check(&tu.name, &input_json_str, ctx.iteration);
        match (status, action) {
            (LoopStatus::Ok, _) => {}
            (LoopStatus::Warning, Some(LoopAction::Warn)) => {
                warn!(
                    tool = %tu.name,
                    iteration = ctx.iteration,
                    "Loop detector near threshold; continuing"
                );
            }
            (LoopStatus::Detected, Some(LoopAction::RequirePermission)) => {
                warn!(
                    tool = %tu.name,
                    iteration = ctx.iteration,
                    "Doom loop detected (3 identical calls); RequirePermission \
                     not yet wired into StreamBuilder, blocking execution as \
                     a safe default"
                );
                doom_loop_tool = Some(tu.name.clone());
                loop_detected = true;
                break;
            }
            (LoopStatus::Detected, _) => {
                loop_detected = true;
                break;
            }
            (LoopStatus::Warning, _) => {
                // Other warning variants are informational; continue.
            }
        }
    }
    if loop_detected {
        ctx.set_end_reason(SessionEndReason::LoopDetected);
        let reason = if let Some(tool) = doom_loop_tool {
            format!("doom loop detected on tool '{tool}'")
        } else {
            "loop detected".to_string()
        };
        Some(reason)
    } else {
        None
    }
}
