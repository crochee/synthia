//! LLM-callable `self_reflect` tool backed by the Guardian layer.
//!
//! The tool accepts no parameters. When invoked it runs an independent
//! context review over the conversation history exposed through
//! [`ToolExecutionContext::messages`] and returns structured feedback.

use std::sync::Arc;

use async_trait::async_trait;
use synthia_guardian::{
    SELF_REFLECT_TOOL_NAME,
    run_self_reflect,
    self_reflect_tool_description,
    self_reflect_tool_parameters,
};
use synthia_provider::traits::ModelProvider;
use synthia_tool::{Tool, ToolInput, ToolOutput};

/// Tool that dispatches `self_reflect` calls to the Guardian review logic.
pub struct SelfReflectTool {
    provider: Arc<dyn ModelProvider>,
    model: String,
}

impl SelfReflectTool {
    pub fn new(
        provider: Arc<dyn ModelProvider>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            provider,
            model: model.into(),
        }
    }
}

#[async_trait]
impl Tool for SelfReflectTool {
    fn name(&self) -> &str {
        SELF_REFLECT_TOOL_NAME
    }

    fn description(&self) -> &str {
        self_reflect_tool_description()
    }

    fn parameters(&self) -> serde_json::Value {
        self_reflect_tool_parameters()
    }

    fn requires_permission(&self) -> bool {
        false
    }

    fn is_concurrency_safe(&self) -> bool {
        true
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        match run_self_reflect(
            &input.context.messages,
            &self.provider,
            &self.model,
        )
        .await
        {
            Ok(result) => {
                let text = serde_json::to_string_pretty(&result)
                    .unwrap_or_else(|_| result.summary.clone());
                ToolOutput::text(text)
            }
            Err(e) => ToolOutput::error(format!(
                "Guardian self-reflection failed: {}",
                e
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, sync::Arc};

    use synthia_provider::types::Message;
    use synthia_tool::types::ToolExecutionContext;

    use super::*;

    struct FakeProvider {
        response: String,
    }

    #[async_trait::async_trait]
    impl ModelProvider for FakeProvider {
        async fn initialize(
            &mut self,
            _config: synthia_provider::types::ProviderConfig,
        ) -> Result<(), synthia_core::Error> {
            Ok(())
        }

        fn name(&self) -> &str {
            "fake"
        }

        fn model_config(&self) -> synthia_provider::types::ModelConfig {
            synthia_provider::types::ModelConfig {
                name: "fake-model".to_string(),
                provider: "fake".to_string(),
                context_window: 128_000,
                max_output_tokens: 4096,
                supports_tools: true,
                supports_streaming: true,
                supports_reasoning: false,
            }
        }

        async fn complete(
            &self,
            _request: synthia_provider::types::CompletionRequest,
        ) -> Result<
            synthia_provider::types::CompletionResponse,
            synthia_core::Error,
        > {
            Ok(synthia_provider::types::CompletionResponse {
                id: "fake".to_string(),
                model: "fake-model".to_string(),
                content: synthia_provider::types::Content::text(
                    self.response.clone(),
                ),
                usage: Default::default(),
                cached: false,
            })
        }

        async fn complete_with_stream(
            &self,
            _request: synthia_provider::types::CompletionRequest,
            _cancel_token: Option<tokio_util::sync::CancellationToken>,
            mut _on_delta: Box<
                dyn FnMut(synthia_provider::types::StreamChunk) + Send,
            >,
        ) -> Result<
            synthia_provider::types::CompletionResponse,
            synthia_core::Error,
        > {
            unimplemented!()
        }

        async fn embed(
            &self,
            _texts: Vec<String>,
        ) -> Result<Vec<Vec<f64>>, synthia_core::Error> {
            Ok(vec![vec![0.0; 1536]; _texts.len()])
        }
    }

    fn make_input() -> ToolInput {
        ToolInput {
            name: SELF_REFLECT_TOOL_NAME.to_string(),
            input: serde_json::json!({}),
            context: ToolExecutionContext::new(
                "session-1".to_string(),
                PathBuf::from("/tmp"),
            )
            .with_messages(vec![Message::user("hello")]),
        }
    }

    #[tokio::test]
    async fn tool_dispatches_to_guardian_review_logic() {
        let response =
            r#"{"summary":"ok","issues":["i1"],"suggestions":["s1"]}"#;
        let tool = SelfReflectTool::new(
            Arc::new(FakeProvider {
                response: response.to_string(),
            }),
            "fake-model",
        );

        let output = tool.call(make_input()).await;
        assert!(
            output.is_error.is_none() || output.is_error == Some(false),
            "expected success, got error: {:?}",
            output
        );
        let text: String =
            output.content.iter().filter_map(|p| p.text()).collect();
        assert!(text.contains("ok"));
        assert!(text.contains("i1"));
        assert!(text.contains("s1"));
    }

    #[tokio::test]
    async fn tool_returns_error_on_invalid_json() {
        let tool = SelfReflectTool::new(
            Arc::new(FakeProvider {
                response: "not json".to_string(),
            }),
            "fake-model",
        );

        let output = tool.call(make_input()).await;
        assert_eq!(output.is_error, Some(true));
    }
}
