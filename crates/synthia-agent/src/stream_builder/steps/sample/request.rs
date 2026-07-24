//! `CompletionRequest` builder.
//!
//! The provider's request shape is a thin wrapper around
//! `AgentConfig` (model / temperature / max_tokens) and the
//! current `LoopContext` messages. Centralised here so the
//! `StepSample` orchestrator stays free of request-shaping noise
//! and so the request can be re-built (e.g. for the
//! [`super::fallback::synchronous_fallback`] call after a stream
//! closes early) without re-running the truncate step.
//!
//! Kept separate from [`super::core`] (the orchestrator) and
//! [`super::truncate`] (which mutates `ctx.messages` before the
//! request is built).

use std::sync::Arc;

use synthia_provider::types::{CompletionRequest, ToolChoice, ToolDefinition};

use crate::{config::AgentConfig, loop_context::LoopContext};

/// Build the `CompletionRequest` from the agent config + the
/// (already-truncated) `LoopContext` messages + the tool list
/// the orchestrator passed in.
pub(super) fn build_completion_request(
    config: &AgentConfig,
    ctx: &LoopContext,
    tools: Vec<ToolDefinition>,
) -> CompletionRequest {
    CompletionRequest {
        model: config.model.clone(),
        messages: Arc::new(ctx.messages.clone()),
        tools: Arc::new(tools),
        tool_choice: ToolChoice::Auto,
        temperature: config.temperature,
        max_tokens: Some(config.max_tokens),
        ..Default::default()
    }
}
