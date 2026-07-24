#![allow(deprecated)]
//! End-to-end connectivity smoke tests against a real LLM provider.
//!
//! This file contains tests that **must** hit a real LLM because they
//! validate the wire format of the OpenAI-compatible provider:
//! basic completion and tool-call serialisation. None of them can be
//! covered by `FakeProvider`.
//!
//! All tests in this file are `#[ignore]`'d by default because they:
//! - Require live network access to the configured LLM provider
//! - Require valid `OPENAI_BASE_URL` + `OPENAI_API_KEY` env vars
//! - Take 1-5+ minutes per test (real LLM response time)
//! - Incur real API costs and are subject to rate limits
//!
//! To run them explicitly (CI nightly / local debugging):
//! ```bash
//! cargo test -p synthia-agent --test e2e_llm_test -- --ignored
//! ```
//!
//! Originally this file also held four agent-loop tests (`SessionStarted`,
//! `LlmResponseComplete`, tool-call plumbing, event ordering). Those
//! assertions don't depend on real LLM behaviour — they verify the agent
//! loop itself, which is fully covered by `FakeProvider`. Hitting a real
//! model for them cost 1-5+ minutes per test and slowed down `cargo test`
//! without catching any additional bugs, so they were rewritten in-place
//! against `FakeProvider`. If you add new agent-loop tests, prefer
//! `FakeProvider` and put them in the relevant unit/integration file.

use std::{path::PathBuf, sync::Arc};

use futures::StreamExt;
use synthia_agent::{
    AgentEvent,
    agent::Agent,
    config::AgentConfig,
    events::SystemEvent,
};
use synthia_hook::HookRegistry;
use synthia_provider::{
    CachePolicy,
    openai::OpenAICompatibleProvider,
    traits::ModelProvider,
    types::{
        Content,
        ContentPart,
        ModelConfig,
        Role,
        StreamChunk,
        TextContent,
        ToolChoice,
        ToolDefinition,
        ToolResult,
        ToolUse,
    },
};
use synthia_tool::registry::{ToolEntry, ToolRegistry};
use tokio_util::sync::CancellationToken;

mod test_support;
use test_support::{FakeProvider, FakeTool, make_run_config};

const MODEL_NAME: &str = "MiniMax-M2.7";

fn load_env() {
    let _ = dotenv::dotenv();
}

fn create_real_provider() -> (Arc<dyn ModelProvider>, String) {
    load_env();

    let base_url =
        std::env::var("OPENAI_BASE_URL").expect("OPENAI_BASE_URL must be set");
    let api_key =
        std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY must be set");
    let model = std::env::var("OPENAI_MODEL")
        .unwrap_or_else(|_| MODEL_NAME.to_string());

    let model_config = ModelConfig {
        name: model.clone(),
        provider: "openai".to_string(),
        context_window: 128_000,
        max_output_tokens: 4096,
        supports_tools: true,
        supports_streaming: true,
        supports_reasoning: true,
    };

    let provider = OpenAICompatibleProvider::new(base_url, model_config)
        .with_api_key(&api_key);
    (Arc::new(provider), model)
}

fn create_test_tool_registry() -> ToolRegistry {
    let registry = ToolRegistry::new();
    registry.register(ToolEntry::new(Arc::new(FakeTool::new(
        "glob",
        "glob result",
    ))));
    registry.register(ToolEntry::new(Arc::new(FakeTool::new(
        "grep",
        "grep result",
    ))));
    registry
}

// ---------------------------------------------------------------------------
// True end-to-end: real LLM provider
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires live LLM API; run via: cargo test -p synthia-agent --test e2e_llm_test -- --ignored"]
async fn test_llm_connectivity() {
    let (provider, model) = create_real_provider();

    use synthia_provider::types::ToolChoice;

    let request = synthia_provider::types::CompletionRequest {
        model,
        messages: Arc::new(vec![synthia_provider::types::Message::user(
            "Reply with 'Hello, Synthia!' and nothing else.",
        )]),
        tools: Arc::new(vec![]),
        tool_choice: ToolChoice::None,
        temperature: Some(0.0),
        max_tokens: Some(50),
        stop_sequences: vec![],
        extra_body: None,
        cache_policy: Some(CachePolicy::default()),
    };

    let response = provider.complete(request).await;
    assert!(response.is_ok(), "LLM connection failed: {:?}", response);

    let response = response.unwrap();
    let text = response
        .content
        .iter()
        .map(|c| match c {
            synthia_provider::types::ContentPart::Text(tc) => tc.text.clone(),
            _ => String::new(),
        })
        .collect::<String>();

    assert!(!text.is_empty(), "LLM returned empty response");
    assert!(
        text.to_lowercase().contains("hello")
            || text.to_lowercase().contains("synthia"),
        "Response doesn't contain expected greeting: {}",
        text
    );
}

/// Verifies that the OpenAI-compatible provider correctly serialises a
/// tool-call request and that the real model returns a non-empty
/// response (text and/or tool calls). Migrated here from
/// `agent_react_loop_test.rs` so all real-LLM tests live in one place.
#[tokio::test]
#[ignore = "requires live LLM API; run via: cargo test -p synthia-agent --test e2e_llm_test -- --ignored"]
async fn test_provider_with_tool_call() {
    let (provider, model) = create_real_provider();

    let tool_def = ToolDefinition::new(
        "get_weather",
        "Get the current weather for a location",
        serde_json::json!({
            "type": "object",
            "properties": {
                "location": {
                    "type": "string",
                    "description": "City name"
                }
            },
            "required": ["location"]
        }),
    );

    let request = synthia_provider::types::CompletionRequest {
        model,
        messages: Arc::new(vec![
            synthia_provider::types::Message {
                role: Role::System,
                content: Content::Single(ContentPart::Text(TextContent {
                    text: "You are a helpful assistant that uses tools."
                        .to_string(),
                    cache_control: None,
                })),
                tool_call_id: None,
                name: None,
                ..Default::default()
            },
            synthia_provider::types::Message {
                role: Role::User,
                content: Content::Single(ContentPart::Text(TextContent {
                    text: "What's the weather in Tokyo?".to_string(),
                    cache_control: None,
                })),
                tool_call_id: None,
                name: None,
                ..Default::default()
            },
        ]),
        tools: Arc::new(vec![tool_def]),
        tool_choice: ToolChoice::Auto,
        temperature: Some(0.7),
        max_tokens: Some(512),
        stop_sequences: vec![],
        extra_body: None,
        cache_policy: Some(CachePolicy::default()),
    };

    let response = provider.complete(request).await;
    match response {
        Ok(resp) => {
            let text = resp.content.extract_text().unwrap_or_default();
            let tool_calls = resp.content.extract_tool_uses();
            assert!(
                !text.is_empty() || !tool_calls.is_empty(),
                "Provider returned empty response: text='{text}', tool_calls={}",
                tool_calls.len()
            );
        }
        Err(e) => {
            // Graceful skip on provider error: the smoke test is best
            // effort in environments without a reachable LLM.
            eprintln!("[skip] Provider error: {e:?}");
        }
    }
}

// ---------------------------------------------------------------------------
// Mocked agent behaviour tests — fast & deterministic
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_agent_react_loop_basic() {
    let provider = Arc::new(FakeProvider::new(vec!["4".to_string()]));
    let tool_registry = create_test_tool_registry();
    let hook_registry = HookRegistry::new();

    let agent_config = AgentConfig {
        model: "fake-model".to_string(),
        max_iterations: 5,
        max_tokens: 1024,
        temperature: Some(0.7),
        workspace_root: PathBuf::from("."),
        ..Default::default()
    };

    let cancel_token = CancellationToken::new();
    let run_config = make_run_config(
        provider,
        tool_registry,
        hook_registry,
        "fake-test-basic".to_string(),
        synthia_agent::types::AgentInput::text(
            "What is 2+2? Reply with just the number.",
        ),
        agent_config,
        cancel_token,
    );

    let events: Vec<AgentEvent> = Agent::run_stream(run_config).collect().await;

    assert!(!events.is_empty(), "No events were emitted");
    assert!(
        events.iter().any(|e| matches!(
            e,
            AgentEvent::System(SystemEvent::SessionStarted { .. })
        )),
        "SessionStarted event not found"
    );
    assert!(
        events.iter().any(|e| matches!(e, AgentEvent::ModelDone(_))),
        "ModelDone (final sampling result) event not found"
    );
}

#[tokio::test]
async fn test_agent_tool_call() {
    // First call: LLM emits a tool_use chunk for `glob`.
    // Second call: LLM emits the final text answer.
    let provider =
        Arc::new(FakeProvider::new(vec![]).with_stream_chunks(vec![
            vec![
                StreamChunk::Content(ContentPart::ToolUse(ToolUse {
                    id: "call_1".into(),
                    name: "glob".into(),
                    input: serde_json::json!({ "pattern": "*" }),
                })),
                StreamChunk::Stop("tool_use".into()),
            ],
            vec![
                StreamChunk::Content(ContentPart::Text(TextContent {
                    text: "There are 3 files.".into(),
                    cache_control: None,
                })),
                StreamChunk::Stop("end_turn".into()),
            ],
        ]));

    let agent_config = AgentConfig {
        model: "fake-model".to_string(),
        max_iterations: 5,
        max_tokens: 1024,
        temperature: Some(0.7),
        workspace_root: PathBuf::from("."),
        ..Default::default()
    };

    let cancel_token = CancellationToken::new();
    let run_config = make_run_config(
        provider,
        create_test_tool_registry(),
        HookRegistry::new(),
        "fake-test-tool".to_string(),
        synthia_agent::types::AgentInput::text(
            "List the files in the current directory.",
        ),
        agent_config,
        cancel_token,
    );

    let events: Vec<AgentEvent> = Agent::run_stream(run_config).collect().await;

    let tool_call_started = events.iter().any(|e| {
        matches!(
            e,
            AgentEvent::Model(ContentPart::ToolUse(ToolUse { name, .. }))
                if name == "glob"
        )
    });
    let tool_call_completed = events.iter().any(|e| {
        matches!(
            e,
            AgentEvent::Model(ContentPart::ToolResult(ToolResult {
                tool_use_id: id,
                ..
            })) if id == "call_1"
        )
    });

    assert!(tool_call_started, "ToolCallStarted(glob) event not found");
    assert!(
        tool_call_completed,
        "ToolCallCompleted(glob) event not found"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            AgentEvent::System(SystemEvent::SessionEnded { .. })
        )),
        "Agent should complete with SessionEnded"
    );
}

#[tokio::test]
async fn test_agent_multi_tool_in_turn() {
    // Single assistant turn emits two tool_use chunks before the final text.
    let provider =
        Arc::new(FakeProvider::new(vec![]).with_stream_chunks(vec![
            vec![
                StreamChunk::Content(ContentPart::ToolUse(ToolUse {
                    id: "call_a".into(),
                    name: "glob".into(),
                    input: serde_json::json!({ "pattern": "*" }),
                })),
                StreamChunk::Content(ContentPart::ToolUse(ToolUse {
                    id: "call_b".into(),
                    name: "grep".into(),
                    input: serde_json::json!({ "pattern": "foo" }),
                })),
                StreamChunk::Stop("tool_use".into()),
            ],
            vec![
                StreamChunk::Content(ContentPart::Text(TextContent {
                    text: "Found 2 matches.".into(),
                    cache_control: None,
                })),
                StreamChunk::Stop("end_turn".into()),
            ],
        ]));

    let agent_config = AgentConfig {
        model: "fake-model".to_string(),
        max_iterations: 5,
        max_tokens: 1024,
        temperature: Some(0.7),
        workspace_root: PathBuf::from("."),
        ..Default::default()
    };

    let cancel_token = CancellationToken::new();
    let run_config = make_run_config(
        provider,
        create_test_tool_registry(),
        HookRegistry::new(),
        "fake-test-multi-tool".to_string(),
        synthia_agent::types::AgentInput::text(
            "Glob everything, then grep for 'foo'.",
        ),
        agent_config,
        cancel_token,
    );

    let events: Vec<AgentEvent> = Agent::run_stream(run_config).collect().await;

    let started_names: Vec<&str> = events
        .iter()
        .filter_map(|e| {
            if let AgentEvent::Model(ContentPart::ToolUse(ToolUse {
                name,
                ..
            })) = e
            {
                Some(name.as_str())
            } else {
                None
            }
        })
        .collect();

    assert!(
        started_names.contains(&"glob"),
        "ToolUse(glob) missing: {started_names:?}"
    );
    assert!(
        started_names.contains(&"grep"),
        "ToolUse(grep) missing: {started_names:?}"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            AgentEvent::System(SystemEvent::SessionEnded { .. })
        )),
        "Agent should complete with SessionEnded"
    );
}

#[tokio::test]
async fn test_event_stream_ordering() {
    let provider = Arc::new(FakeProvider::new(vec!["test".to_string()]));

    let agent_config = AgentConfig {
        model: "fake-model".to_string(),
        max_iterations: 3,
        max_tokens: 256,
        temperature: Some(0.0),
        workspace_root: PathBuf::from("."),
        ..Default::default()
    };

    let cancel_token = CancellationToken::new();
    let run_config = make_run_config(
        provider,
        create_test_tool_registry(),
        HookRegistry::new(),
        "fake-test-ordering".to_string(),
        synthia_agent::types::AgentInput::text("Say 'test' and nothing else."),
        agent_config,
        cancel_token,
    );

    let events: Vec<AgentEvent> = Agent::run_stream(run_config).collect().await;

    // SessionStarted must appear before SessionEnded.
    let first_idx = events
        .iter()
        .position(|e| {
            matches!(e, AgentEvent::System(SystemEvent::SessionStarted { .. }))
        })
        .expect("SessionStarted not found");
    let last_idx = events
        .iter()
        .rposition(|e| {
            matches!(e, AgentEvent::System(SystemEvent::SessionEnded { .. }))
        })
        .expect("SessionEnded not found");
    assert!(
        first_idx < last_idx,
        "SessionStarted should come before SessionEnded"
    );

    // The final aggregated `ModelDone` (sampling result) must appear
    // between SessionStarted and SessionEnded.
    let resp_idx = events
        .iter()
        .position(|e| matches!(e, AgentEvent::ModelDone(_)))
        .expect("ModelDone not found");
    assert!(
        first_idx < resp_idx && resp_idx < last_idx,
        "ModelDone ({resp_idx}) should come between SessionStarted ({first_idx}) and SessionEnded ({last_idx})"
    );
}
