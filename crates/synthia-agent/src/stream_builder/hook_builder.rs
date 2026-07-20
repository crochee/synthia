use std::sync::Arc;

use synthia_core::Error;
use synthia_hook::{AgentContext, HookRegistry};

/// HookBuilder — 事件驱动的 hook 分发器（逐步废弃）。
///
/// # Migration
///
/// New code should use [`crate::InterceptorChain`] (middleware pattern)
/// instead of `HookBuilder` (event-driven pattern). InterceptorChain
/// supports short-circuit, retry, and approval logic that HookBuilder
/// cannot express. See the `synthia-agent-composition-a2a` OpenSpec change.
#[derive(Clone)]
pub struct HookBuilder {
    registry: Arc<HookRegistry>,
}

impl HookBuilder {
    pub fn new(registry: Arc<HookRegistry>) -> Self {
        Self { registry }
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
