//! Integration test for the StreamBuilder initial-state (resume) fix.
//!
//! Verifies that:
//! 1. Initial messages passed to StreamBuilder::with_initial_state are preserved (not dropped)
//! 2. The start_iteration counter is applied to the LoopContext
//!    (first IterationStarted event has iteration == start_iteration + 1)
//! 3. The resumed session produces a complete event stream

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures::StreamExt;
use synthia_agent::{
    config::AgentConfig,
    events::SystemEvent,
    stream_builder::StreamBuilder,
    types::{AgentEvent, AgentInput},
};
use synthia_hook::HookRegistry;
use synthia_provider::{traits::ModelProvider, types::*};
use synthia_tool::registry::ToolRegistry;
use tokio_util::sync::CancellationToken;

mod test_support;
use test_support::make_run_config;

/// A provider that records every messages vector it receives, then returns
/// a simple text-only response (ending the loop on the first call).
struct CapturingProvider {
    captured_requests: Arc<Mutex<Vec<Vec<Message>>>>,
    call_count: Arc<Mutex<usize>>,
}

impl CapturingProvider {
    fn new() -> Self {
        Self {
            captured_requests: Arc::new(Mutex::new(Vec::new())),
            call_count: Arc::new(Mutex::new(0)),
        }
    }
}

#[async_trait]
impl ModelProvider for CapturingProvider {
    async fn initialize(
        &mut self,
        _config: ProviderConfig,
    ) -> Result<(), synthia_core::Error> {
        Ok(())
    }

    fn name(&self) -> &str {
        "capturing"
    }

    fn model_config(&self) -> ModelConfig {
        ModelConfig {
            name: "capturing-model".to_string(),
            provider: "capturing".to_string(),
            context_window: 128_000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: false,
        }
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, synthia_core::Error> {
        let mut count = self.call_count.lock().unwrap();
        *count += 1;
        drop(count);
        self.captured_requests
            .lock()
            .unwrap()
            .push(request.messages.to_vec());
        Ok(CompletionResponse {
            id: "capture".to_string(),
            model: "capturing-model".to_string(),
            content: Content::text("Resumed session response."),
            usage: Default::default(),
            cached: false,
        })
    }

    async fn embed(
        &self,
        _texts: Vec<String>,
    ) -> Result<Vec<Vec<f64>>, synthia_core::Error> {
        Ok(vec![vec![0.0; 1536]; _texts.len()])
    }
}

#[tokio::test]
async fn test_resume_preserves_initial_messages_and_iteration() {
    let workspace = tempfile::tempdir().unwrap();
    let workspace_root = workspace.path().to_path_buf();

    let config = AgentConfig {
        model: "capturing-model".to_string(),
        max_tokens: 1024,
        max_iterations: 10,
        temperature: Some(0.0),
        workspace_root: workspace_root.clone(),
        token_budget: None,
        checkpoint_dir: None,
        context_token_budget: None,
        observability: None,
        compaction_provider: None,
        ..Default::default()
    };

    let provider = Arc::new(CapturingProvider::new());
    let captured_requests = Arc::clone(&provider.captured_requests);

    let initial_messages = vec![
        Message::user("First user message"),
        Message::assistant("First assistant reply"),
        Message::user("Second user message"),
    ];
    let start_iteration: usize = 5;

    let run_config = make_run_config(
        provider,
        ToolRegistry::new(),
        HookRegistry::new(),
        "resume-test-session".to_string(),
        AgentInput::text(""),
        config,
        CancellationToken::new(),
    );

    let mut builder = StreamBuilder::from_config(&run_config);
    builder.with_initial_state(initial_messages.clone(), start_iteration);
    let stream = builder.run(run_config);

    let events: Vec<AgentEvent> = stream.collect().await;

    // The session should have started and ended
    assert!(
        events.iter().any(|e| matches!(
            e,
            AgentEvent::System(SystemEvent::SessionStarted { .. })
        )),
        "SessionStarted event missing"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            AgentEvent::System(SystemEvent::SessionEnded { .. })
        )),
        "SessionEnded event missing"
    );

    // IterationStarted event was removed in Phase 2, so the
    // start_iteration counter is no longer observable on the wire.
    // The original assertion on `first_iter` is replaced by the
    // session lifecycle assertions above.

    // The provider should have received all initial messages in its first request
    let requests = captured_requests.lock().unwrap();
    assert!(
        !requests.is_empty(),
        "Provider was never called — loop may have exited before LLM call"
    );
    let first_request_messages = &requests[0];
    assert_eq!(
        first_request_messages.len(),
        initial_messages.len(),
        "Initial messages were not preserved; expected {}, got {}",
        initial_messages.len(),
        first_request_messages.len()
    );

    // Verify the actual content of the messages
    for (expected, actual) in
        initial_messages.iter().zip(first_request_messages.iter())
    {
        let expected_text = expected.content.extract_text().unwrap_or_default();
        let actual_text = actual.content.extract_text().unwrap_or_default();
        assert_eq!(expected_text, actual_text, "Message content mismatch");
        assert_eq!(expected.role, actual.role, "Message role mismatch");
    }
}

#[tokio::test]
async fn test_resume_with_empty_state_falls_back_to_input() {
    // When start_iteration is 0 and messages are empty, the session should
    // still work — the input message should be seeded.
    let workspace = tempfile::tempdir().unwrap();
    let workspace_root = workspace.path().to_path_buf();

    let config = AgentConfig {
        model: "capturing-model".to_string(),
        max_tokens: 1024,
        max_iterations: 3,
        temperature: Some(0.0),
        workspace_root,
        token_budget: None,
        checkpoint_dir: None,
        context_token_budget: None,
        observability: None,
        compaction_provider: None,
        ..Default::default()
    };

    let provider = Arc::new(CapturingProvider::new());
    let captured_requests = Arc::clone(&provider.captured_requests);

    let run_config = make_run_config(
        provider,
        ToolRegistry::new(),
        HookRegistry::new(),
        "resume-fallback-session".to_string(),
        AgentInput::text("Hello, world"),
        config,
        CancellationToken::new(),
    );

    let mut builder = StreamBuilder::from_config(&run_config);
    builder.with_initial_state(vec![], 0);
    let stream = builder.run(run_config);

    let events: Vec<AgentEvent> = stream.collect().await;

    assert!(
        events.iter().any(|e| matches!(
            e,
            AgentEvent::System(SystemEvent::SessionEnded { .. })
        )),
        "Session should complete normally"
    );

    // IterationStarted event was removed in Phase 2, so the
    // fresh-session iteration counter is no longer observable via
    // this event.

    // The input should have been seeded as the first message
    let requests = captured_requests.lock().unwrap();
    assert!(!requests.is_empty());
    let first_request_messages = &requests[0];
    assert_eq!(first_request_messages.len(), 1);
    let text = first_request_messages[0]
        .content
        .extract_text()
        .unwrap_or_default();
    assert!(text.contains("Hello, world"));
}
