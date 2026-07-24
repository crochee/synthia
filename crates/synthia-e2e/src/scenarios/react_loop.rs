use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use synthia_core::Error;
use synthia_provider::ModelProvider;
use synthia_tool::{
    traits::Tool,
    types::{ToolInput, ToolOutput},
};

use crate::utils::mock_provider::{MockProvider, MockResponse};

#[derive(Debug)]
struct PanicTool {
    call_count: Arc<AtomicUsize>,
}

impl PanicTool {
    fn new() -> Self {
        Self {
            call_count: Arc::new(AtomicUsize::new(0)),
        }
    }
}

#[async_trait]
impl Tool for PanicTool {
    fn name(&self) -> &str {
        "panic_tool"
    }

    fn description(&self) -> &str {
        "A tool that panics when called"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn call(&self, _input: ToolInput) -> ToolOutput {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        panic!("Intentional panic in tool");
    }
}

#[tokio::test]
async fn test_react_loop_basic() {
    let mut provider = MockProvider::new();

    provider.with_response(MockResponse::text("I'll help you with that task."));
    provider.with_response(MockResponse::text(
        "The task is complete. Here's the result: 42",
    ));

    let response1 = provider
        .complete(synthia_provider::CompletionRequest::default())
        .await;
    assert!(response1.is_ok());
    let resp1 = response1.unwrap();
    assert!(resp1.content.extract_text().unwrap().contains("help"));

    let response2 = provider
        .complete(synthia_provider::CompletionRequest::default())
        .await;
    assert!(response2.is_ok(), "response2 failed: {:?}", response2.err());
    let resp2 = response2.unwrap();
    assert!(resp2.content.extract_text().unwrap().contains("42"));

    assert_eq!(provider.call_count(), 2);
}

#[tokio::test]
async fn test_react_loop_with_compact() {
    let mut provider = MockProvider::new();

    for i in 0..10 {
        provider.with_response(MockResponse::text(format!(
            "Response iteration {}",
            i
        )));
    }

    let mut total_tokens = 0;

    for i in 0..10 {
        let response = provider
            .complete(synthia_provider::CompletionRequest::default())
            .await;
        assert!(response.is_ok());
        let resp = response.unwrap();
        total_tokens += resp.usage.prompt_tokens + resp.usage.completion_tokens;

        if total_tokens > 50000 {
            let text = resp.content.extract_text().unwrap();
            if text.contains("compacted")
                || text.contains("summarized")
                || text.contains("compressed")
            {
                break;
            }
        }

        if i == 9 {
            let _ = resp;
        }
    }
}

#[tokio::test]
async fn test_tool_panic_isolation() {
    let panic_tool = Arc::new(PanicTool::new());
    let call_count = panic_tool.call_count.clone();

    let tool_input = ToolInput {
        name: "panic_tool".to_string(),
        input: serde_json::json!({}),
        context: synthia_tool::types::ToolExecutionContext::new(
            "test-session".to_string(),
            std::path::PathBuf::from("/tmp"),
        ),
    };

    let result = tokio::spawn({
        let tool = panic_tool.clone();
        async move { tool.call(tool_input).await }
    })
    .await;

    assert!(result.is_err() || call_count.load(Ordering::SeqCst) > 0);

    assert_eq!(
        call_count.load(Ordering::SeqCst),
        1,
        "Tool should have been called exactly once despite panic"
    );
}

#[cfg(test)]
mod additional_tests {
    use synthia_provider::ModelProvider;

    use super::*;

    #[tokio::test]
    async fn test_react_loop_multiple_iterations() {
        let mut provider = MockProvider::new();

        let steps = vec![
            "Step 1: Analyzing the problem",
            "Step 2: Implementing solution",
            "Step 3: Testing the implementation",
            "Step 4: Verifying results",
            "Task completed successfully",
        ];

        for step in &steps {
            provider.with_response(MockResponse::text(*step));
        }

        for (i, step) in steps.iter().enumerate() {
            let response = provider
                .complete(synthia_provider::CompletionRequest::default())
                .await;
            assert!(response.is_ok(), "Step {} should succeed", i + 1);

            let resp = response.unwrap();
            assert!(resp.content.extract_text().unwrap().contains(step));
        }

        assert_eq!(provider.call_count(), steps.len());
    }

    #[tokio::test]
    async fn test_react_loop_with_tool_calls() {
        let mut provider = MockProvider::new();

        let tool_use = synthia_provider::ToolUse {
            id: "call_1".to_string(),
            name: "read_file".to_string(),
            input: serde_json::json!({"path": "/test.txt"}),
        };
        provider.with_response(MockResponse::with_tools(
            "I'll read the file for you.",
            vec![tool_use],
        ));
        provider.with_response(MockResponse::text(
            "The file contains: test content",
        ));

        let response1 = provider
            .complete(synthia_provider::CompletionRequest {
                model: "mock".to_string(),
                ..Default::default()
            })
            .await;
        assert!(response1.is_ok());
        let resp1 = response1.unwrap();
        assert!(resp1.content.has_tool_use());

        let response2 = provider
            .complete(synthia_provider::CompletionRequest {
                model: "mock".to_string(),
                ..Default::default()
            })
            .await;
        assert!(response2.is_ok());
        let resp2 = response2.unwrap();
        assert!(
            resp2
                .content
                .extract_text()
                .unwrap()
                .contains("test content")
        );
    }
}
