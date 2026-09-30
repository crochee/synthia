//! End-to-end delegation tests: a parent `ReActAgent` with the
//! [`TaskDelegator`] interceptor installed runs a registered peer
//! as a child and commits its answer as the `task` tool result.
//!
//! Fixtures are local copies of the agent crate's test providers
//! (`CapturingProvider` / `EchoChildProvider`) — the plugin crate
//! owns its own harness, exactly as a downstream consumer would.

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use async_trait::async_trait;
use futures::StreamExt;
use synthia_core::{CancelToken, Error, registry::Registry};
use synthia_harness::{
    Agent,
    AgentEntry,
    AgentEvent,
    AgentInput,
    AgentRegistry,
    PromptContext,
    ReActAgent,
    SystemEvent,
};
use synthia_provider::{
    CompletionResponse,
    ContentPart,
    ProviderConfig,
    Role,
    SamplingResult,
    StreamChunk,
    TextContent,
    TokenUsage,
    ToolDefinition,
    ToolUse,
    traits::ModelProvider,
    types::{CompletionRequest, ModelConfig},
};
use synthia_tool::ToolRegistry;
use synthia_tool_task::{MAX_SUBAGENT_DEPTH, TaskDelegator};
use tokio::sync::Mutex as TokioMutex;
use tokio_util::sync::CancellationToken;

/// Provider that records every `CompletionRequest` (messages +
/// tool definitions) and replays a scripted chunk sequence —
/// used to assert what the loop actually sends to the model.
struct CapturingProvider {
    captured_tools: Arc<TokioMutex<Vec<Arc<Vec<ToolDefinition>>>>>,
    scripted: Arc<TokioMutex<Vec<Vec<StreamChunk>>>>,
}

impl CapturingProvider {
    fn new(scripted: Vec<Vec<StreamChunk>>) -> Self {
        Self {
            captured_tools: Arc::new(TokioMutex::new(Vec::new())),
            scripted: Arc::new(TokioMutex::new(scripted)),
        }
    }
}

#[async_trait]
impl ModelProvider for CapturingProvider {
    async fn initialize(
        &mut self,
        _config: ProviderConfig,
    ) -> Result<(), Error> {
        Ok(())
    }

    fn name(&self) -> &str {
        "capturing"
    }

    fn model_config(&self) -> ModelConfig {
        ModelConfig {
            name: "capturing".into(),
            provider: "test".into(),
            context_window: 128_000,
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
        unreachable!("streaming path")
    }

    async fn complete_with_stream(
        &self,
        request: CompletionRequest,
        _cancel: Option<Arc<dyn CancelToken>>,
        mut on_delta: Box<dyn FnMut(StreamChunk) + Send>,
    ) -> Result<CompletionResponse, Error> {
        self.captured_tools.lock().await.push(request.tools.clone());
        let chunks = {
            let mut g = self.scripted.lock().await;
            if g.is_empty() {
                vec![StreamChunk::IsDone {
                    result: Box::new(SamplingResult::default()),
                }]
            } else {
                g.remove(0)
            }
        };
        let mut sampling: Option<SamplingResult> = None;
        for c in chunks {
            if let StreamChunk::IsDone { result } = &c {
                sampling = Some((**result).clone());
            }
            on_delta(c);
        }
        let s = sampling.unwrap_or_default();
        Ok(CompletionResponse {
            id: "cap".into(),
            model: "capturing".into(),
            content: synthia_provider::Content::Single(ContentPart::Text(
                TextContent {
                    text: s.text.clone(),
                    cache_control: None,
                },
            )),
            usage: s.usage.clone(),
            cached: false,
            replay_state: None,
            stop_reason: s.stop_reason.clone(),
        })
    }
}

/// Provider stub for the child agent: echoes the trailing user
/// prompt (skipping the runtime-context snapshot frame) back as
/// the child's answer so tests can assert prompt propagation.
#[derive(Clone)]
struct EchoChildProvider {
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl ModelProvider for EchoChildProvider {
    async fn initialize(
        &mut self,
        _config: ProviderConfig,
    ) -> Result<(), Error> {
        Ok(())
    }

    fn name(&self) -> &str {
        "echo-child"
    }

    fn model_config(&self) -> ModelConfig {
        ModelConfig {
            name: "echo-child".to_string(),
            provider: "test".to_string(),
            context_window: 128_000,
            max_output_tokens: 1024,
            supports_tools: false,
            supports_streaming: true,
            supports_reasoning: false,
        }
    }

    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> Result<CompletionResponse, Error> {
        unreachable!("streaming path")
    }

    async fn complete_with_stream(
        &self,
        request: CompletionRequest,
        _cancel_token: Option<Arc<dyn CancelToken>>,
        mut on_delta: Box<dyn FnMut(StreamChunk) + Send>,
    ) -> Result<CompletionResponse, Error> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let snapshot_prefix = "Current runtime context. \
            This snapshot supersedes earlier runtime-context \
            snapshots.";
        let user_text = request
            .messages
            .iter()
            .rev()
            .filter(|m| m.role == Role::User)
            .find_map(|m| match &m.content {
                synthia_provider::Content::Single(ContentPart::Text(t)) => {
                    if t.text.starts_with(snapshot_prefix) {
                        None
                    } else {
                        Some(t.text.clone())
                    }
                }
                _ => None,
            })
            .unwrap_or_default();
        let text = format!("child-answered: {user_text}");
        on_delta(StreamChunk::IsDone {
            result: Box::new(SamplingResult {
                text: text.clone(),
                tool_calls: vec![],
                reasoning: String::new(),
                reasoning_signature: None,
                usage: TokenUsage::default(),
                ..Default::default()
            }),
        });
        Ok(CompletionResponse {
            id: "child".into(),
            model: "echo-child".into(),
            content: synthia_provider::Content::Single(ContentPart::Text(
                TextContent {
                    text,
                    cache_control: None,
                },
            )),
            usage: TokenUsage::default(),
            cached: false,
            replay_state: None,
            stop_reason: None,
        })
    }
}

fn task_call(id: &str, agent: &str, prompt: &str) -> Vec<StreamChunk> {
    vec![StreamChunk::IsDone {
        result: Box::new(SamplingResult {
            text: String::new(),
            tool_calls: vec![ToolUse {
                id: id.into(),
                name: "task".into(),
                input: serde_json::json!({
                    "agent": agent,
                    "prompt": prompt
                }),
            }],
            reasoning: String::new(),
            reasoning_signature: None,
            usage: TokenUsage::default(),
            ..Default::default()
        }),
    }]
}

fn text_only(text: &str) -> Vec<StreamChunk> {
    vec![StreamChunk::IsDone {
        result: Box::new(SamplingResult {
            text: text.into(),
            tool_calls: vec![],
            reasoning: String::new(),
            reasoning_signature: None,
            usage: TokenUsage::default(),
            ..Default::default()
        }),
    }]
}

async fn echo_child_registry() -> AgentRegistry {
    let registry = AgentRegistry::new();
    let mut child = ReActAgent::with_prompt_context(
        Arc::new(EchoChildProvider {
            calls: Arc::new(AtomicUsize::new(0)),
        }),
        Arc::new(ToolRegistry::new()),
        PathBuf::new(),
        "child instructions".to_string(),
        Arc::new(PromptContext::default()),
    );
    let mut child_desc = child.descriptor().clone();
    child_desc.name = "child".into();
    child.descriptor_mut(child_desc);
    registry
        .put(AgentEntry::new(Arc::new(child)))
        .await
        .expect("register child");
    registry
}

#[tokio::test]
async fn task_tool_delegates_to_peer_and_wraps_child_events() {
    // Parent: first completion issues ONE `task` tool call to
    // peer `"child"`; second completion is text-only so the
    // loop terminates. The child echoes its user prompt.
    let parent_provider = Arc::new(CapturingProvider::new(vec![
        task_call("call_task_1", "child", "explore the topic"),
        text_only("done"),
    ]));

    let child_registry = echo_child_registry().await;

    let parent = ReActAgent::with_prompt_context(
        parent_provider.clone(),
        Arc::new(ToolRegistry::new()),
        PathBuf::new(),
        "parent instructions".to_string(),
        Arc::new(PromptContext::default()),
    )
    .with_interceptor(Arc::new(TaskDelegator::new(Arc::new(child_registry))));

    let mut events = Vec::new();
    let mut stream = parent
        .run(AgentInput::text("hi"), Arc::new(CancellationToken::new()))
        .await;
    while let Some(ev) = stream.next().await {
        events.push(ev);
    }
    drop(stream);

    // The parent tool list must have carried the `task` tool
    // definition into the LLM request.
    let captured_tools = parent_provider.captured_tools.lock().await;
    let first_tools = captured_tools.first().expect("parent request tools");
    let tool_names: Vec<&str> =
        first_tools.iter().map(|d| d.name.as_str()).collect();
    assert!(
        tool_names.contains(&"task"),
        "parent tool list must include the task tool; got {tool_names:?}"
    );

    // Child events must surface wrapped in AgentEvent::Agent
    // with the child's AgentMeta (depth 1 = direct sub-agent).
    let child_meta_ok = events.iter().any(|e| {
        matches!(e, AgentEvent::Agent(meta, inner)
            if meta.parent_depth == 1
                && !meta.child_session_id.is_empty()
                && matches!(&**inner, AgentEvent::System(SystemEvent::SessionStarted{..})))
    });
    assert!(
        child_meta_ok,
        "child SessionStarted must be wrapped with parent_depth=1 and a stable child trace id; got {events:?}"
    );

    // The child's final text must reach the parent history as
    // the `task` tool result.
    let tool_result_text = events.iter().any(|e| {
        matches!(e, AgentEvent::Model(ContentPart::ToolResult(tr))
        if tr.tool_name.as_deref() == Some("task")
            && tr.content.iter().any(|p| {
                matches!(p, ContentPart::Text(t)
                    if t.text.contains("child-answered: explore the topic"))
            }))
    });
    assert!(
        tool_result_text,
        "child answer must be committed as the task tool result"
    );

    // The parent's final output must be the second completion's
    // text.
    assert!(
        events.iter().any(|e| {
            matches!(e, AgentEvent::Model(ContentPart::Text(t))
                    if t.text == "done")
        }),
        "parent must finish with its own completion text"
    );
}

#[tokio::test]
async fn task_tool_unknown_agent_returns_error_result() {
    let parent_provider = Arc::new(CapturingProvider::new(vec![
        task_call("call_task_x", "ghost", "hi"),
        text_only("recovered"),
    ]));

    let parent = ReActAgent::with_prompt_context(
        parent_provider.clone(),
        Arc::new(ToolRegistry::new()),
        PathBuf::new(),
        String::new(),
        Arc::new(PromptContext::default()),
    )
    .with_interceptor(Arc::new(TaskDelegator::new(Arc::new(
        AgentRegistry::new(),
    ))));

    let mut saw_error_result = false;
    let mut stream = parent
        .run(AgentInput::text("hi"), Arc::new(CancellationToken::new()))
        .await;
    while let Some(ev) = stream.next().await {
        if let AgentEvent::Model(ContentPart::ToolResult(tr)) = &ev
            && tr.tool_name.as_deref() == Some("task")
            && tr.content.iter().any(|p| {
                matches!(p, ContentPart::Text(t)
                        if t.text.contains("unknown peer agent `ghost`"))
            })
        {
            saw_error_result = true;
        }
    }
    drop(stream);
    assert!(
        saw_error_result,
        "unknown agent must surface as an error tool result"
    );
}

#[tokio::test]
async fn task_tool_depth_guard_refuses_spawn_at_limit() {
    // A run at MAX_SUBAGENT_DEPTH (a sub-agent of a sub-agent
    // of a sub-agent) must refuse further delegation with an
    // error tool result instead of recursing forever.
    let parent_provider = Arc::new(CapturingProvider::new(vec![
        task_call("call_task_d", "child", "deeper"),
        text_only("ok"),
    ]));

    // Register one child so the ONLY possible rejection is the
    // depth guard, not an unknown-agent error.
    let child_registry = echo_child_registry().await;

    let parent = ReActAgent::with_prompt_context(
        parent_provider.clone(),
        Arc::new(ToolRegistry::new()),
        PathBuf::new(),
        String::new(),
        Arc::new(PromptContext::default()),
    )
    .with_interceptor(Arc::new(TaskDelegator::new(Arc::new(child_registry))));

    let mut input = AgentInput::text("hi");
    input.subagent_depth = MAX_SUBAGENT_DEPTH;

    let mut saw_depth_error = false;
    let mut stream =
        parent.run(input, Arc::new(CancellationToken::new())).await;
    while let Some(ev) = stream.next().await {
        if let AgentEvent::Model(ContentPart::ToolResult(tr)) = &ev
            && tr.tool_name.as_deref() == Some("task")
            && tr.content.iter().any(|p| {
                matches!(p, ContentPart::Text(t)
                        if t.text.contains("maximum sub-agent depth"))
            })
        {
            saw_depth_error = true;
        }
    }
    drop(stream);
    assert!(
        saw_depth_error,
        "depth-limited delegation must surface the depth error"
    );
}

#[tokio::test]
async fn task_tool_absent_without_the_interceptor() {
    // No delegator installed → the `task` definition must NOT
    // appear in the tool list; the loop stays a plain harness.
    let parent_provider = Arc::new(CapturingProvider::new(vec![
        task_call("call_task_n", "child", "hi"),
        text_only("ok"),
    ]));

    let parent = ReActAgent::with_prompt_context(
        parent_provider.clone(),
        Arc::new(ToolRegistry::new()),
        PathBuf::new(),
        String::new(),
        Arc::new(PromptContext::default()),
    );

    let mut events = Vec::new();
    let mut stream = parent
        .run(AgentInput::text("hi"), Arc::new(CancellationToken::new()))
        .await;
    while let Some(ev) = stream.next().await {
        events.push(ev);
    }
    drop(stream);

    let captured_tools = parent_provider.captured_tools.lock().await;
    let first_tools = captured_tools.first().expect("parent request tools");
    assert!(
        !first_tools.iter().any(|d| d.name == "task"),
        "task tool must be absent when no delegator is installed"
    );
    let _ = events;
}
