//! The three `on_*` methods covering domain-specific events that
//! need a hook reaction but are not part of the canonical lifecycle.
//!
//! - [`HookExecutor::on_tool_error`]: synthetic `after_tool` event
//!   with `{"status": "error", "error": <error_string>}` payload.
//! - [`HookExecutor::on_loop_detected`]: synthetic `before_tool` event
//!   with `{"loop_type": <loop_type>}` payload; returns a
//!   [`ToolAction`] verdict (Proceed is the safe default).
//! - [`HookExecutor::on_session_end`]: `complete` event fired at
//!   session shutdown (distinct from the agent-loop `complete`).

use synthia_hook::{AgentContext, ToolAction};
use tracing::warn;

use super::{catch_unwind::catch_unwind, executor::HookExecutor};

impl HookExecutor {
    pub async fn on_tool_error(
        &self,
        ctx: &AgentContext,
        tool_call: &serde_json::Value,
        error: &str,
    ) {
        let err_result = serde_json::json!({"status": "error", "error": error});
        match catch_unwind(async {
            self.registry
                .fire_after_tool(ctx, tool_call, &err_result)
                .await
        })
        .await
        {
            Ok(_) => {}
            Err(_) => {
                warn!("Hook on_tool_error panicked (fail-open)");
            }
        }
    }

    pub async fn on_loop_detected(
        &self,
        ctx: &AgentContext,
        loop_type: &str,
    ) -> ToolAction {
        match catch_unwind(async {
            let loop_info = serde_json::json!({"loop_type": loop_type});
            self.registry.fire_before_tool(ctx, &loop_info).await
        })
        .await
        {
            Ok(result) => result,
            Err(_) => {
                warn!(
                    "Hook loop_detected panicked (fail-open), returning Proceed"
                );
                ToolAction::Proceed
            }
        }
    }

    pub async fn on_session_end(&self, ctx: &AgentContext) {
        match catch_unwind(async { self.registry.fire_complete(ctx).await })
            .await
        {
            Ok(_) => {}
            Err(_) => {
                warn!("Hook on_session_end panicked (fail-open)");
            }
        }
    }
}
