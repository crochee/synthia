//! Initialization helpers for the per-iteration loop.

use synthia_provider::types::Message;
use synthia_session::store::SessionInputQueue;

use crate::{
    events::{AgentEvent, HookEvent},
    input::AgentInput,
    loop_context::LoopContext,
};

/// Seed the context with the input message or the
/// checkpoint-resumed state.
///
/// Resume semantics: treat `initial_state` as a resume
/// only when there are actual messages OR
/// `iteration > 0`. Otherwise the empty
/// `(vec![], 0)` state would clobber the user input.
pub(crate) fn seed_initial_messages(
    ctx: &mut LoopContext,
    initial_state: Option<&(Vec<Message>, usize)>,
    input: &AgentInput,
) {
    if let Some((msgs, iter)) = initial_state {
        if !msgs.is_empty() || *iter > 0 {
            ctx.messages = msgs.clone();
            ctx.iteration = *iter;
        } else if ctx.messages.is_empty() {
            ctx.messages.push(input.to_message());
        }
    } else if ctx.messages.is_empty() {
        ctx.messages.push(input.to_message());
    }
}

/// Drain the persisted steering-input queue at the start
/// of an iteration.
///
/// Returns one `SteeringReceived` event per drained
/// message. Each drained message is prepended to the
/// context (position 0) so the LLM sees it as the most
/// recent user turn.
pub(crate) async fn drain_steering(
    ctx: &mut LoopContext,
    input_queue: Option<&SessionInputQueue>,
    user_id: &str,
    session_id: &str,
) -> Vec<AgentEvent> {
    let Some(queue) = input_queue else {
        return Vec::new();
    };
    let mut events = Vec::new();
    let Ok(pending) = queue.drain_pending(user_id, session_id) else {
        return Vec::new();
    };
    for input in pending {
        events.push(AgentEvent::Hook(HookEvent::Message {
            priority: input.priority as i32,
            message: input.content.clone(),
        }));
        ctx.messages.insert(0, Message::user(input.content));
    }
    events
}
