//! The six `fire_*` methods covering the canonical agent lifecycle:
//! `before_llm` / `before_tool` / `after_tool` / `after_llm` /
//! `iteration_end` / `complete`.
//!
//! Every method calls [`super::catch_unwind::catch_unwind`] around
//! the underlying [`HookRegistry`] dispatch. If the hook panics or
//! returns `Err`, we log a warning and either return `()` (for
//! fire-and-forget hooks) or [`ToolAction::Proceed`] (for the two
//! verdict-returning hooks: `before_tool` and `on_loop_detected`).

use synthia_hook::{AgentContext, ToolAction};
use tracing::{instrument, warn};

use super::{catch_unwind::catch_unwind, executor::HookExecutor};

impl HookExecutor {
    #[instrument(skip(self, ctx), fields(hook = "before_llm"))]
    pub async fn fire_before_llm(&self, ctx: &mut AgentContext) {
        match catch_unwind(async { self.registry.fire_before_llm(ctx).await })
            .await
        {
            Ok(_) => {}
            Err(_) => {
                warn!("Hook before_llm panicked (fail-open)");
            }
        }
    }

    #[instrument(skip(self, ctx), fields(hook = "before_tool"))]
    pub async fn fire_before_tool(
        &self,
        ctx: &AgentContext,
        tool_call: &serde_json::Value,
    ) -> ToolAction {
        match catch_unwind(async {
            self.registry.fire_before_tool(ctx, tool_call).await
        })
        .await
        {
            Ok(result) => result,
            Err(_) => {
                warn!(
                    "Hook before_tool panicked (fail-open), returning Proceed"
                );
                ToolAction::Proceed
            }
        }
    }

    #[instrument(skip(self, ctx), fields(hook = "after_tool"))]
    pub async fn fire_after_tool(
        &self,
        ctx: &AgentContext,
        tool_call: &serde_json::Value,
        result: &serde_json::Value,
    ) {
        match catch_unwind(async {
            self.registry.fire_after_tool(ctx, tool_call, result).await
        })
        .await
        {
            Ok(_) => {}
            Err(_) => {
                warn!("Hook after_tool panicked (fail-open)");
            }
        }
    }

    #[instrument(skip(self, ctx), fields(hook = "after_llm"))]
    pub async fn fire_after_llm(
        &self,
        ctx: &AgentContext,
        response: &serde_json::Value,
    ) {
        match catch_unwind(async {
            self.registry.fire_after_llm(ctx, response).await
        })
        .await
        {
            Ok(_) => {}
            Err(_) => {
                warn!("Hook after_llm panicked (fail-open)");
            }
        }
    }

    #[instrument(skip(self, ctx), fields(hook = "iteration_end"))]
    pub async fn fire_iteration_end(
        &self,
        ctx: &AgentContext,
        iteration: usize,
    ) {
        match catch_unwind(async {
            self.registry.fire_iteration_end(ctx, iteration).await
        })
        .await
        {
            Ok(_) => {}
            Err(_) => {
                warn!("Hook iteration_end panicked (fail-open)");
            }
        }
    }

    #[instrument(skip(self, ctx), fields(hook = "complete"))]
    pub async fn fire_complete(&self, ctx: &AgentContext) {
        match catch_unwind(async { self.registry.fire_complete(ctx).await })
            .await
        {
            Ok(_) => {}
            Err(_) => {
                warn!("Hook complete panicked (fail-open)");
            }
        }
    }
}
