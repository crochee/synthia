use std::sync::Arc;

use synthia_core::Error;
use synthia_hook::{AgentContext, HookRegistry};

#[derive(Clone)]
pub struct HookBuilder {
    registry: Arc<HookRegistry>,
}

impl HookBuilder {
    pub fn new(registry: Arc<HookRegistry>) -> Self {
        Self { registry }
    }

    #[deprecated(
        note = "Use UnifiedHookDispatcher::dispatch(HookEvent::UserPromptSubmit) instead. Will be removed after 6-month deprecation window."
    )]
    pub async fn fire_before_llm(
        &self,
        ctx: &mut AgentContext,
    ) -> Result<(), Error> {
        self.registry.fire_before_llm(ctx).await
    }

    #[deprecated(
        note = "Use UnifiedHookDispatcher::dispatch(HookEvent::PostResponse) instead. Will be removed after 6-month deprecation window."
    )]
    pub async fn fire_after_llm(
        &self,
        ctx: &AgentContext,
        response: &serde_json::Value,
    ) -> Result<(), Error> {
        self.registry.fire_after_llm(ctx, response).await
    }

    #[deprecated(
        note = "Use UnifiedHookDispatcher::dispatch(HookEvent::PreToolUse) instead. Will be removed after 6-month deprecation window."
    )]
    pub async fn fire_before_tool(
        &self,
        ctx: &AgentContext,
        call_json: &serde_json::Value,
    ) -> Result<synthia_hook::ToolAction, Error> {
        self.registry.fire_before_tool(ctx, call_json).await
    }

    #[deprecated(
        note = "Use UnifiedHookDispatcher::dispatch(HookEvent::PostToolUse) instead. Will be removed after 6-month deprecation window."
    )]
    pub async fn fire_after_tool(
        &self,
        ctx: &AgentContext,
        call_json: &serde_json::Value,
        result_json: &serde_json::Value,
    ) -> Result<(), Error> {
        self.registry
            .fire_after_tool(ctx, call_json, result_json)
            .await
    }

    pub async fn fire_iteration_end(
        &self,
        ctx: &AgentContext,
        iteration: usize,
    ) -> Result<(), Error> {
        self.registry.fire_iteration_end(ctx, iteration).await
    }

    pub async fn fire_complete(&self, ctx: &AgentContext) -> Result<(), Error> {
        self.registry.fire_complete(ctx).await
    }

    pub fn get_registry(&self) -> &HookRegistry {
        &self.registry
    }
}
