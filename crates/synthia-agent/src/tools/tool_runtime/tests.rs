use std::sync::Arc;

use async_trait::async_trait;
use synthia_tool_orchestrator::{
    ExecutionContext,
    ToolCallRequest,
    ToolCallResult,
    ToolOrchestrator,
    ToolOrchestratorError,
};

use super::ToolRuntime;

struct MockOrchestrator;

#[async_trait]
impl ToolOrchestrator for MockOrchestrator {
    async fn execute(
        &self,
        request: ToolCallRequest,
        _context: ExecutionContext,
        _cancel: tokio_util::sync::CancellationToken,
    ) -> Result<ToolCallResult, ToolOrchestratorError> {
        Ok(ToolCallResult {
            call_id: request.call_id,
            tool_name: request.tool_name,
            outcome: serde_json::json!({"ok": true}),
            is_error: false,
            tool_id: request.tool_id,
        })
    }

    async fn execute_batch(
        &self,
        requests: Vec<ToolCallRequest>,
        context: ExecutionContext,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<Vec<ToolCallResult>, ToolOrchestratorError> {
        let mut results = Vec::new();
        for req in requests {
            let result =
                self.execute(req, context.clone(), cancel.clone()).await?;
            results.push(result);
        }
        Ok(results)
    }

    fn event_stream(
        &self,
    ) -> tokio::sync::broadcast::Receiver<
        synthia_tool_orchestrator::ToolOrchestratorEvent,
    > {
        let (tx, rx) = tokio::sync::broadcast::channel(256);
        let _ = tx;
        rx
    }

    async fn cancel(
        &self,
        _call_id: &str,
    ) -> Result<(), ToolOrchestratorError> {
        Ok(())
    }
}

#[tokio::test]
async fn tool_runtime_execute_batch() {
    let orchestrator: Arc<dyn ToolOrchestrator> = Arc::new(MockOrchestrator);
    let extension_manager =
        super::super::dynamic_provider::ExtensionManager::new();
    let runtime = ToolRuntime::new(orchestrator, extension_manager);

    let requests = vec![ToolCallRequest {
        call_id: "call-1".to_string(),
        tool_name: "test_tool".to_string(),
        arguments: serde_json::json!({}),
        permission: synthia_permission::Permission::AutoApprove,
        tool_id: None,
    }];

    let context = ExecutionContext {
        session_id: "session-1".to_string(),
        workspace_root: std::path::PathBuf::from("/tmp"),
        caller_agent: "test".to_string(),
    };

    let cancel = tokio_util::sync::CancellationToken::new();
    let results = runtime
        .execute_batch(requests, context, cancel)
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].call_id, "call-1");
}

#[tokio::test]
async fn tool_runtime_extension_manager_access() {
    let orchestrator: Arc<dyn ToolOrchestrator> = Arc::new(MockOrchestrator);
    let extension_manager =
        super::super::dynamic_provider::ExtensionManager::new();
    let runtime = ToolRuntime::new(orchestrator, extension_manager);

    assert!(runtime.extension_manager().is_empty());
}
