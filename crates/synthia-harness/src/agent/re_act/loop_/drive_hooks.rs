//! Cross-cutting helpers used by [`super::drive`].
//!
//! Three concerns live here, all called from the per-iteration
//! or run-level phases of the orchestrator:
//!
//! - [`hooks_on_agent_start`] — fan out every `AgentHook::OnAgentStart`
//!   over the session's prompt text; hook failures become warning
//!   events, never run-fatal errors.
//! - [`hooks_on_agent_end`] — same fan-out, with the run's final
//!   message and end reason.
//! - [`cancelled`] — the loop-wide "is the run cancelled?" check
//!   that also emits a `SessionInterrupted` warning when the
//!   answer flips.
//!
//! These three are pulled into their own file because they are
//! the only helpers called from more than one phase (the two
//! `hooks_*` are called by [`super::drive_setup`] and
//! [`super::drive_finalize`]; [`cancelled`] is called from the
//! iteration phases). Keeping them in `drive.rs` would mix
//! the phase bodies with their cross-cutting helpers.

use std::sync::Arc;

use synthia_steering::{HookError, HookStage, run_hook};
use tracing::{info, warn};

use super::ReActLoop;
use crate::events::{AgentOutput, SessionEndReason, SystemEvent, WarningKind};

/// Fan every hook's `OnAgentStart` over the session's prompt
/// text. Hook-runner failures surface as warning events,
/// never as run-fatal errors.
pub(super) async fn hooks_on_agent_start(this: &ReActLoop, prompt_text: &str) {
    for hook in &this.steering.hooks {
        let prompt_text = prompt_text.to_string();
        if let Err(err) =
            run_hook(&**hook, HookStage::OnAgentStart, move || {
                let hook = Arc::clone(hook);
                let prompt_text = prompt_text.clone();
                Box::pin(async move {
                    hook.on_agent_start(&prompt_text).await;
                    Ok::<_, HookError>(())
                })
            })
            .await
        {
            this.sink.system(SystemEvent::Warning {
                kind: WarningKind::Hook,
                message: err.to_string(),
                iteration: None,
            });
        }
    }
}

/// Fan every hook's `OnAgentEnd` with the final message and
/// reason. Hook-runner failures surface as warning events.
pub(super) async fn hooks_on_agent_end(
    this: &ReActLoop,
    output: &AgentOutput,
    reason: &SessionEndReason,
) {
    let duration = this.started_at.elapsed();
    for hook in &this.steering.hooks {
        let final_message = output.final_message.clone();
        let reason_str = reason.as_str().to_string();
        if let Err(err) = run_hook(&**hook, HookStage::OnAgentEnd, move || {
            let hook = Arc::clone(hook);
            let final_message = final_message.clone();
            let reason_str = reason_str.clone();
            Box::pin(async move {
                hook.on_agent_end(
                    final_message.as_deref(),
                    &reason_str,
                    duration,
                )
                .await;
                Ok::<_, HookError>(())
            })
        })
        .await
        {
            this.sink.system(SystemEvent::Warning {
                kind: WarningKind::Hook,
                message: err.to_string(),
                iteration: None,
            });
        }
    }
}

/// Cancel-aware warning emission. Returns `true` iff the run
/// was cancelled; emits `SessionInterrupted` on the way out.
pub(super) fn cancelled(this: &ReActLoop, reason: &str) -> bool {
    if !this.cancel.is_cancelled() {
        return false;
    }
    info!("emitting SessionInterrupted: {}", reason);
    this.sink.system(SystemEvent::SessionInterrupted {
        reason: reason.to_string(),
    });
    warn!("session interrupted: {}", reason);
    true
}
