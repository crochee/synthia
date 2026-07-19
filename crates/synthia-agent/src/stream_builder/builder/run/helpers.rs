//! Hook outcome handling helpers for the main loop.
//!
//! Provides [`handle_hook_outcome`] for processing
//! [`synthia_hook::HookOutcome`] from
//! [`synthia_hook::UnifiedHookDispatcher::dispatch`] and
//! [`emit_turn_event`] for best-effort JSONL event persistence.

use std::sync::Arc;

use synthia_session::store::EventStore;

use crate::{events::append_agent_event, turn::TurnId};

/// Best-effort append of a durable JSONL event for the current turn.
///
/// Errors are logged but never abort the agent loop, matching the
/// streaming semantics of the surrounding code.
#[allow(clippy::too_many_arguments)]
pub(super) async fn emit_turn_event<P>(
    event_store: &EventStore,
    session_store: &synthia_session::Store,
    user_id: &str,
    session_id: &str,
    event_type: &str,
    turn_id: TurnId,
    iteration: usize,
    payload: P,
) where
    P: serde::Serialize + Send + 'static,
{
    let path = session_store.session_dir(user_id, session_id);
    if let Err(e) = append_agent_event(
        event_store,
        &path,
        session_id,
        event_type,
        turn_id,
        iteration,
        payload,
    )
    .await
    {
        tracing::warn!(
            session_id = %session_id,
            error = %e,
            event_type = %event_type,
            "Failed to append turn event to JSONL log"
        );
    }
}

/// Handle a [`synthia_hook::HookOutcome`] from
/// [`synthia_hook::UnifiedHookDispatcher::dispatch`].
///
/// - `Allow`: no-op
/// - `Deny { reason }`: log a warning
/// - `ForwardToMainAgent { hint }`: inject into the steering channel
///   (if present and below [`crate::steering::FORWARDED_RATE_LIMIT`])
///
/// Returns `true` if a forwarded message was actually injected.
pub(super) async fn handle_hook_outcome(
    outcome: &synthia_hook::HookOutcome,
    steering_channel: &Option<Arc<dyn crate::steering::SteeringChannel>>,
    forwarded_count: &mut usize,
) -> bool {
    match outcome {
        synthia_hook::HookOutcome::Allow => false,
        synthia_hook::HookOutcome::Deny { reason } => {
            tracing::warn!(reason = %reason, "hook denied event");
            false
        }
        synthia_hook::HookOutcome::ForwardToMainAgent { hint } => {
            if *forwarded_count >= crate::steering::FORWARDED_RATE_LIMIT {
                tracing::warn!(
                    hint = %hint,
                    forwarded_count,
                    limit = crate::steering::FORWARDED_RATE_LIMIT,
                    "forward_to_main_agent rate limit exceeded, dropping"
                );
                return false;
            }
            if let Some(channel) = steering_channel {
                channel
                    .send(crate::steering::SteeringMessage::forwarded(hint))
                    .await;
                *forwarded_count += 1;
                tracing::debug!(
                    hint = %hint,
                    forwarded_count,
                    "forwarded hook outcome to main agent steering channel"
                );
                true
            } else {
                tracing::debug!(
                    hint = %hint,
                    "no steering channel, dropping forwarded hook outcome"
                );
                false
            }
        }
    }
}
