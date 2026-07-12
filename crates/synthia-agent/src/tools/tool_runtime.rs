//! Tool runtime — orchestrates tool execution with hooks, retries, and parallel execution.

use std::sync::Arc;

use synthia_tool_orchestrator::{
    ToolCallRequest,
    ToolCallResult,
    ToolOrchestrator,
    ToolOrchestratorError,
};

#[cfg(test)]
mod tests;

/// Runtime that orchestrates tool execution across multiple providers.
#[derive(Clone)]
pub struct ToolRuntime {
    orchestrator: Arc<dyn ToolOrchestrator>,
    extension_manager: super::dynamic_provider::ExtensionManager,
}

impl ToolRuntime {
    pub fn new(
        orchestrator: Arc<dyn ToolOrchestrator>,
        extension_manager: super::dynamic_provider::ExtensionManager,
    ) -> Self {
        Self {
            orchestrator,
            extension_manager,
        }
    }

    /// Execute a batch of tool calls in parallel, using the orchestrator.
    pub async fn execute_batch(
        &self,
        requests: Vec<ToolCallRequest>,
        context: synthia_tool_orchestrator::ExecutionContext,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<Vec<ToolCallResult>, ToolOrchestratorError> {
        self.orchestrator
            .execute_batch(requests, context, cancel)
            .await
    }

    /// Get the extension manager for this runtime.
    pub fn extension_manager(
        &self,
    ) -> &super::dynamic_provider::ExtensionManager {
        &self.extension_manager
    }
}
