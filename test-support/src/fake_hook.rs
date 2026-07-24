use std::sync::Arc;

use async_trait::async_trait;
use synthia_core::Error;
use synthia_hook::traits::{AgentContext, AgentHook, ToolAction};
use tokio::sync::Mutex;

#[derive(Debug)]
pub struct FakeHook {
    invocations: Arc<Mutex<Vec<String>>>,
}

impl FakeHook {
    pub fn new() -> Self {
        Self {
            invocations: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn invocations(&self) -> Vec<String> {
        self.invocations.lock().await.clone()
    }
}

impl Default for FakeHook {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentHook for FakeHook {
    async fn on_before_llm(
        &self,
        _ctx: &mut AgentContext,
    ) -> Result<(), Error> {
        self.invocations
            .lock()
            .await
            .push("on_before_llm".to_string());
        Ok(())
    }

    async fn on_after_llm(
        &self,
        _ctx: &AgentContext,
        _response: &serde_json::Value,
    ) -> Result<(), Error> {
        self.invocations
            .lock()
            .await
            .push("on_after_llm".to_string());
        Ok(())
    }

    async fn on_before_tool(
        &self,
        _ctx: &AgentContext,
        _call: &serde_json::Value,
    ) -> Result<ToolAction, Error> {
        self.invocations
            .lock()
            .await
            .push("on_before_tool".to_string());
        Ok(ToolAction::Proceed)
    }

    async fn on_after_tool(
        &self,
        _ctx: &AgentContext,
        _call: &serde_json::Value,
        _result: &serde_json::Value,
    ) -> Result<(), Error> {
        self.invocations
            .lock()
            .await
            .push("on_after_tool".to_string());
        Ok(())
    }

    async fn on_iteration_end(
        &self,
        _ctx: &AgentContext,
        _iteration: usize,
    ) -> Result<(), Error> {
        self.invocations
            .lock()
            .await
            .push("on_iteration_end".to_string());
        Ok(())
    }

    async fn on_complete(&self, _ctx: &AgentContext) -> Result<(), Error> {
        self.invocations
            .lock()
            .await
            .push("on_complete".to_string());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fake_hook_records_invocations() {
        let hook = FakeHook::new();
        let mut ctx = AgentContext::new("s1".to_string(), "t1".to_string());

        hook.on_before_llm(&mut ctx).await.unwrap();
        hook.on_after_llm(&ctx, &serde_json::json!({}))
            .await
            .unwrap();
        hook.on_before_tool(&ctx, &serde_json::json!({}))
            .await
            .unwrap();
        hook.on_after_tool(
            &ctx,
            &serde_json::json!({}),
            &serde_json::json!({}),
        )
        .await
        .unwrap();
        hook.on_iteration_end(&ctx, 0).await.unwrap();
        hook.on_complete(&ctx).await.unwrap();

        let invocations = hook.invocations().await;
        assert_eq!(
            invocations,
            vec![
                "on_before_llm",
                "on_after_llm",
                "on_before_tool",
                "on_after_tool",
                "on_iteration_end",
                "on_complete",
            ]
        );
    }
}
