//! The one-pass request the single-shot strategies share.
//!
//! Chain-of-Thought and best-of-N ask the runtime the same question —
//! one tool-free completion over the caller's history, fitted by the
//! runtime's window policy — and differ only in how many times they ask
//! it and what they do with the answers. Building that request in one
//! place keeps the two from drifting on the three decisions that make it
//! what it is: the window is fitted by the shared
//! [`ContextManager`](synthia_context::ContextManager), **no** tools are
//! offered, and the output cap is the model's own.

use std::sync::Arc;

use synthia_context::AgentState;
use synthia_provider::{CompletionRequest, Content, Message, Role, ToolChoice};

use crate::{agent::strategy::AgentRuntime, input::AgentInput};

/// Build the request one completion is asked with.
///
/// The system prompt is the run's assembled prompt (the caller's
/// [`PromptContext`](crate::prompt::PromptContext) when the input
/// carries one, the runtime's otherwise); `instruction` is appended to
/// it for strategies that steer through the prompt (Chain-of-Thought's
/// step format) rather than through tools.
///
/// Deliberately tool-free: a strategy that wants tools is the ReAct
/// loop, and mixing the two would make "the answer" depend on which of
/// several samples happened to call a tool.
pub(crate) async fn single_shot_request(
    runtime: &AgentRuntime,
    input: &AgentInput,
    instruction: Option<&str>,
) -> CompletionRequest {
    let base = input
        .prompt_context
        .as_ref()
        .map(|ctx| ctx.assemble(&runtime.descriptor))
        .unwrap_or_else(|| runtime.system_prompt());
    let system = match instruction {
        Some(instruction) => format!("{base}\n\n{instruction}"),
        None => base,
    };

    let mut messages = Vec::with_capacity(input.history.len() + 2);
    messages.push(Message::system(system));
    messages.extend(input.history.iter().cloned());
    messages.push(Message::new(
        Role::User,
        Content::parts(input.content.clone()),
    ));

    // The runtime's window policy runs once per request: every sample
    // asks the same question, so they share the fitted list.
    let config = runtime.model_config();
    let mut state = AgentState::from_config(&config);
    let fitted = runtime
        .context_manager
        .prepare_arc(Arc::new(messages), &mut state)
        .await;

    CompletionRequest {
        model: config.name.clone(),
        messages: fitted,
        tools: Arc::new(Vec::new()),
        tool_choice: ToolChoice::None,
        max_tokens: Some(config.max_output_tokens),
        ..CompletionRequest::default()
    }
}
