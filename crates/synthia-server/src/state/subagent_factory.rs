//! Server-side implementation of [`SubagentSessionFactory`].
//!
//! Backed by [`AppState`], this factory creates real child sessions
//! through the session manager and ensures a [`SessionController`] is
//! running for the child so events can be streamed back to clients.

use std::{sync::Weak, time::Duration};

use async_trait::async_trait;
use synthia_agent::{
    AgentEvent,
    ChildSessionHandle,
    SubagentSessionError,
    SubagentSessionFactory,
    agent_instance::{AgentResult, AgentStatus, AgentTokenUsage},
    events::SessionEndReason,
    truncate_summary,
};

use super::app_state::AppState;
use crate::session::controller::SessionOp;

/// Factory that creates child sessions using the server's [`AppState`].
#[derive(Clone, Debug)]
pub struct AppStateSubagentFactory {
    state: Weak<AppState>,
}

impl AppStateSubagentFactory {
    pub fn new(state: Weak<AppState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl SubagentSessionFactory for AppStateSubagentFactory {
    async fn create_child(
        &self,
        user_id: String,
        parent_session_id: String,
        maybe_id: Option<String>,
        parent_depth: usize,
    ) -> Result<ChildSessionHandle, SubagentSessionError> {
        let state = self.state.upgrade().ok_or_else(|| {
            SubagentSessionError::CreationFailed("AppState dropped".to_string())
        })?;

        // Ensure the parent has a running controller and grab its
        // forwarded-event sender so the child can mirror events back.
        let parent_controller = state
            .get_or_create_session_controller(&user_id, &parent_session_id)
            .await
            .map_err(|e| {
                SubagentSessionError::CreationFailed(format!("{e:?}"))
            })?;
        let parent_event_sender = parent_controller.event_sender();

        let child = state
            .session_manager
            .create_child(user_id.clone(), parent_session_id.clone(), maybe_id)
            .await
            .map_err(|e| {
                SubagentSessionError::CreationFailed(format!("{e:?}"))
            })?;

        // Create the child controller with `parent_depth` so the
        // child's `SubagentManager` is configured with depth
        // `parent_depth + 1`. This closes the depth-propagation gap:
        // `max_depth` enforcement in `AgentTool::call` now works for
        // nested spawns in production (not just the parent side).
        let _controller = state
            .get_or_create_session_controller_with_parent(
                &user_id,
                &child.id,
                Some(parent_event_sender.clone()),
                Some(parent_depth),
            )
            .await
            .map_err(|e| {
                SubagentSessionError::CreationFailed(format!("{e:?}"))
            })?;

        Ok(ChildSessionHandle {
            session_id: child.id,
            user_id,
            parent_event_sender: Some(parent_event_sender),
        })
    }

    async fn run_child(
        &self,
        user_id: String,
        parent_session_id: String,
        prompt: String,
        parent_depth: usize,
        maybe_id: Option<String>,
    ) -> Result<AgentResult, SubagentSessionError> {
        let child = self
            .create_child(
                user_id.clone(),
                parent_session_id.clone(),
                maybe_id,
                parent_depth,
            )
            .await?;

        // Capture the parent's forwarded-event sender and the child
        // session id up front so we can emit a `SubagentCompleted`
        // notification after the run finishes (best-effort).
        let parent_sender = child.parent_event_sender.clone();
        let child_session_id = child.session_id.clone();

        let state = self.state.upgrade().ok_or_else(|| {
            SubagentSessionError::CreationFailed("AppState dropped".to_string())
        })?;

        let controller = state
            .get_or_create_session_controller(&user_id, &child.session_id)
            .await
            .map_err(|e| {
                SubagentSessionError::CreationFailed(format!("{e:?}"))
            })?;

        // Subscribe before enqueuing the prompt so we do not miss the
        // completion events broadcast by the child controller.
        let mut events = controller.subscribe();

        controller
            .submit(SessionOp::Prompt {
                content: prompt,
                priority: 1,
            })
            .await
            .map_err(|e| {
                SubagentSessionError::CreationFailed(format!(
                    "failed to enqueue child prompt: {e:?}"
                ))
            })?;

        let result =
            wait_for_child_completion(&mut events, Duration::from_secs(300))
                .await;

        // Emit `SubagentCompleted` to the parent's event stream
        // (best-effort: a closed or full parent channel must not break
        // the child run). The summary is truncated to 500 chars per
        // the `subagent-background-mode` spec.
        let result_summary = match &result {
            Ok(r) => truncate_summary(&r.output, 500),
            Err(e) => truncate_summary(&e.to_string(), 500),
        };
        let completed = AgentEvent::SubagentCompleted {
            session_id: child_session_id.clone(),
            result_summary,
        };
        let wrapped = AgentEvent::SubagentEvent {
            child_session_id,
            event: Box::new(completed),
        };
        if let Some(sender) = &parent_sender {
            // `try_send` is non-blocking and returns an error if the
            // channel is closed or full; both are acceptable here.
            let _ = sender.try_send(wrapped);
        }

        // The controller may outlive the run; we do not shut it down
        // here so that clients can continue streaming events.
        result
    }
}

/// Wait for the child session to emit a [`SessionEnded`] event, collecting
/// the final output along the way.
async fn wait_for_child_completion(
    events: &mut tokio::sync::broadcast::Receiver<AgentEvent>,
    timeout: Duration,
) -> Result<AgentResult, SubagentSessionError> {
    let mut final_output = String::new();

    let outcome = tokio::time::timeout(timeout, async {
        loop {
            match events.recv().await {
                Ok(AgentEvent::Finish { output }) => {
                    final_output = output;
                }
                Ok(AgentEvent::SessionEnded { reason }) => {
                    return Ok(reason);
                }
                Ok(_) => {
                    // Other lifecycle events are ignored here.
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    return Err(SubagentSessionError::CreationFailed(
                        "child event channel closed before completion"
                            .to_string(),
                    ));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    // We lagged behind; keep waiting for the terminal
                    // event, but do not treat it as fatal.
                }
            }
        }
    })
    .await
    .map_err(|_| {
        SubagentSessionError::CreationFailed(
            "timed out waiting for child session to complete".to_string(),
        )
    })?;

    let reason = outcome?;
    let status = match reason {
        SessionEndReason::Completed => AgentStatus::Completed,
        SessionEndReason::Cancelled => AgentStatus::Cancelled,
        SessionEndReason::Error(_)
        | SessionEndReason::TokenBudgetExceeded
        | SessionEndReason::MaxIterationsReached
        | SessionEndReason::GuardianBlocked
        | SessionEndReason::LoopDetected
        | SessionEndReason::CircuitBreakerOpen => AgentStatus::Errored,
    };

    let output = if status == AgentStatus::Completed && !final_output.is_empty()
    {
        final_output
    } else {
        format!("{reason:?}")
    };

    Ok(AgentResult {
        output,
        status,
        token_usage: AgentTokenUsage {
            input_tokens: 0,
            output_tokens: 0,
        },
    })
}
