use std::{
    collections::HashMap,
    sync::{
        Arc,
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use async_trait::async_trait;
use synthia_core::Error;
use synthia_provider::*;

#[derive(Debug, Clone)]
pub struct MockResponse {
    pub content: Content,
    pub usage: TokenUsage,
}

impl MockResponse {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: Content::text(text),
            usage: TokenUsage::default(),
        }
    }

    pub fn with_tools(text: &str, tools: Vec<ToolUse>) -> Self {
        let mut parts = vec![ContentPart::Text(TextContent {
            text: text.to_string(),
            cache_control: None,
        })];
        for tool in tools {
            parts.push(ContentPart::ToolUse(tool));
        }
        Self {
            content: Content::parts(parts),
            usage: TokenUsage::default(),
        }
    }
}

#[derive(Debug)]
pub struct MockProvider {
    responses: Arc<Mutex<Vec<MockResponse>>>,
    tool_responses: Arc<Mutex<HashMap<String, serde_json::Value>>>,
    should_fail: bool,
    call_count: Arc<AtomicUsize>,
    final_response: Arc<Mutex<Option<String>>>,
}

impl MockProvider {
    pub fn new() -> Self {
        Self {
            responses: Arc::new(Mutex::new(Vec::new())),
            tool_responses: Arc::new(Mutex::new(HashMap::new())),
            should_fail: false,
            call_count: Arc::new(AtomicUsize::new(0)),
            final_response: Arc::new(Mutex::new(None)),
        }
    }

    pub fn with_response(&mut self, response: MockResponse) -> &mut Self {
        self.responses.lock().unwrap().push(response);
        self
    }

    pub fn with_response_text(&mut self, text: &str) -> &mut Self {
        self.with_response(MockResponse::text(text))
    }

    pub fn with_tool_response(
        &mut self,
        tool_name: &str,
        response: serde_json::Value,
    ) -> &mut Self {
        self.tool_responses
            .lock()
            .unwrap()
            .insert(tool_name.to_string(), response);
        self
    }

    pub fn with_final_response(&mut self, response: &str) -> &mut Self {
        *self.final_response.lock().unwrap() = Some(response.to_string());
        self
    }

    pub fn should_fail(&mut self, fail: bool) -> &mut Self {
        self.should_fail = fail;
        self
    }

    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }

    pub fn pop_tool_response(
        &self,
        tool_name: &str,
    ) -> Option<serde_json::Value> {
        self.tool_responses.lock().unwrap().remove(tool_name)
    }
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ModelProvider for MockProvider {
    async fn initialize(
        &mut self,
        _config: ProviderConfig,
    ) -> Result<(), Error> {
        Ok(())
    }

    fn name(&self) -> &str {
        "mock"
    }

    fn model_config(&self) -> ModelConfig {
        ModelConfig {
            name: "mock-model".to_string(),
            provider: "mock".to_string(),
            context_window: 128000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: false,
        }
    }

    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> Result<CompletionResponse, Error> {
        let count = self.call_count.fetch_add(1, Ordering::SeqCst);

        if self.should_fail {
            return Err(Error::RateLimited(None));
        }

        let final_resp = self.final_response.lock().unwrap();
        if let Some(ref response) = *final_resp {
            return Ok(CompletionResponse {
                id: format!("mock-{}", count),
                model: "mock-model".to_string(),
                content: Content::text(response.clone()),
                usage: TokenUsage::default(),
                cached: false,
            });
        }
        drop(final_resp);

        let mut responses = self.responses.lock().unwrap();
        if responses.is_empty() {
            return Err(Error::Provider("No responses configured".to_string()));
        }

        let resp = responses.remove(0);
        Ok(CompletionResponse {
            id: format!("mock-{}", count),
            model: "mock-model".to_string(),
            content: resp.content,
            usage: resp.usage,
            cached: false,
        })
    }

    async fn embed(&self, _texts: Vec<String>) -> Result<Vec<Vec<f64>>, Error> {
        // Return dummy embeddings for E2E testing
        Ok(vec![vec![0.0; 1536]; _texts.len()])
    }
}

#[cfg(test)]
mod tests {
    use synthia_provider::ModelProvider;

    use super::*;

    #[tokio::test]
    async fn test_mock_provider_text_response() {
        let mut provider = MockProvider::new();
        provider.with_response_text("Hello, world!");

        let response = provider.complete(CompletionRequest::default()).await;
        assert!(response.is_ok());
        let resp = response.unwrap();
        assert_eq!(
            resp.content.extract_text(),
            Some("Hello, world!".to_string())
        );
    }

    #[tokio::test]
    async fn test_mock_provider_call_count() {
        let mut provider = MockProvider::new();
        provider.with_response_text("Response 1");
        provider.with_response_text("Response 2");
        provider.with_response_text("Response 3");

        assert_eq!(provider.call_count(), 0);

        provider
            .complete(CompletionRequest::default())
            .await
            .unwrap();
        assert_eq!(provider.call_count(), 1);

        provider
            .complete(CompletionRequest::default())
            .await
            .unwrap();
        assert_eq!(provider.call_count(), 2);
    }

    #[tokio::test]
    async fn test_mock_provider_tool_responses() {
        let mut provider = MockProvider::new();
        provider.with_tool_response(
            "read_file",
            serde_json::json!({"content": "file contents"}),
        );

        let response = provider.pop_tool_response("read_file");
        assert!(response.is_some());
        assert_eq!(response.unwrap()["content"], "file contents");
    }

    #[tokio::test]
    async fn test_mock_provider_with_tools() {
        let mut provider = MockProvider::new();
        let tool_use = ToolUse {
            id: "tool_1".to_string(),
            name: "read_file".to_string(),
            input: serde_json::json!({"path": "/test.txt"}),
        };
        provider.with_response(MockResponse::with_tools(
            "Reading file",
            vec![tool_use],
        ));

        let response = provider.complete(CompletionRequest::default()).await;
        assert!(response.is_ok());
        let resp = response.unwrap();
        assert!(resp.content.has_tool_use());
    }
}
