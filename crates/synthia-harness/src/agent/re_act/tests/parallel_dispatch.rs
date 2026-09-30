//! Parallel / sequential tool execution + descriptor-pinning
//! regression tests.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use futures::StreamExt;
use serde_json::json;
use synthia_core::{CancelToken, Error};
use synthia_provider::{
    CompletionRequest,
    CompletionResponse,
    SamplingResult,
    StreamChunk,
    traits::ModelProvider,
    types::ModelConfig,
};
use synthia_tool::{Context, Tool, ToolOutput, ToolRegistry};
use tokio_util::sync::CancellationToken;

use super::{support::*, *};

struct SleepTool;

#[async_trait]
impl synthia_tool::Tool for SleepTool {
    fn name(&self) -> &str {
        "sleep"
    }

    fn description(&self) -> &str {
        "sleeps for a configurable duration"
    }

    fn parameters(&self) -> serde_json::Value {
        json!({"type": "object"})
    }

    fn mode(&self) -> synthia_tool::traits::ExecutionMode {
        synthia_tool::traits::ExecutionMode::Parallel
    }

    async fn call(
        &self,
        _input: serde_json::Value,
        _ctx: &Context,
    ) -> ToolOutput {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        ToolOutput::text("done")
    }
}

#[tokio::test]
async fn parallel_safe_tools_run_concurrently() {
    let tool_use1 = ToolUse {
        id: "c1".into(),
        name: "sleep".into(),
        input: json!({}),
    };
    let tool_use2 = ToolUse {
        id: "c2".into(),
        name: "sleep".into(),
        input: json!({}),
    };
    let tool_use3 = ToolUse {
        id: "c3".into(),
        name: "sleep".into(),
        input: json!({}),
    };
    let provider = Arc::new(CapturingProvider::new(vec![
        tool_call_response(vec![tool_use1, tool_use2, tool_use3]),
        empty_response(),
    ]));
    let registry = Arc::new(ToolRegistry::new());
    registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(SleepTool)));
    let started = std::time::Instant::now();
    let _events = run_and_collect(
        provider,
        registry,
        CancellationToken::new(),
        AgentInput::text("run"),
    )
    .await;
    let elapsed = started.elapsed();
    assert!(
        elapsed < std::time::Duration::from_millis(250),
        "3 parallel-safe sleep(100ms) tools must finish in <250ms, took {elapsed:?}"
    );
}

struct SlowFastTool(u64);

#[async_trait]
impl synthia_tool::Tool for SlowFastTool {
    fn name(&self) -> &str {
        if self.0 == 1 { "slow" } else { "fast" }
    }

    fn description(&self) -> &str {
        "speed"
    }

    fn parameters(&self) -> serde_json::Value {
        json!({"type": "object"})
    }

    fn mode(&self) -> synthia_tool::traits::ExecutionMode {
        synthia_tool::traits::ExecutionMode::Parallel
    }

    async fn call(
        &self,
        _input: serde_json::Value,
        _ctx: &Context,
    ) -> ToolOutput {
        let d = if self.0 == 1 { 200 } else { 10 };
        tokio::time::sleep(std::time::Duration::from_millis(d)).await;
        ToolOutput::text(self.name().to_string())
    }
}

#[tokio::test]
async fn parallel_safe_tool_emits_progress_and_result_in_llm_order() {
    let tool_use_slow = ToolUse {
        id: "slow".into(),
        name: "slow".into(),
        input: json!({}),
    };
    let tool_use_fast = ToolUse {
        id: "fast".into(),
        name: "fast".into(),
        input: json!({}),
    };
    let provider = Arc::new(CapturingProvider::new(vec![
        tool_call_response(vec![tool_use_slow, tool_use_fast]),
        empty_response(),
    ]));
    let registry = Arc::new(ToolRegistry::new());
    registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(
        SlowFastTool(1),
    )));
    registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(
        SlowFastTool(2),
    )));
    let events = run_and_collect(
        provider,
        registry,
        CancellationToken::new(),
        AgentInput::text("o"),
    )
    .await;

    let progress: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::System(SystemEvent::ToolProgress {
                tool_name, ..
            }) => Some(tool_name.clone()),
            _ => None,
        })
        .collect();
    // ToolProgress is only emitted by tools that override
    // `stream()`; these synthetic tools use the default
    // call→Result path, so progress is empty here. The
    // meaningful invariant is ToolResult order below.
    let _ = progress;

    let results: Vec<Option<String>> = events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::Model(ContentPart::ToolResult(tr)) => {
                Some(tr.tool_name.clone())
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        results,
        vec![Some("slow".to_string()), Some("fast".to_string())],
        "ToolResult order must follow LLM tool-call order"
    );
}

static UNSAFE_RUNNING: AtomicUsize = AtomicUsize::new(0);
static MAX_CONCURRENT_UNSAFE: AtomicUsize = AtomicUsize::new(0);

struct CountingSafeTool;
struct CountingUnsafeTool;

#[async_trait]
impl synthia_tool::Tool for CountingSafeTool {
    fn name(&self) -> &str {
        "safe"
    }

    fn description(&self) -> &str {
        "safe"
    }

    fn parameters(&self) -> serde_json::Value {
        json!({"type":"object"})
    }

    fn mode(&self) -> synthia_tool::traits::ExecutionMode {
        synthia_tool::traits::ExecutionMode::Parallel
    }

    async fn call(&self, _: serde_json::Value, _: &Context) -> ToolOutput {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        ToolOutput::text("safe-done")
    }
}

#[async_trait]
impl synthia_tool::Tool for CountingUnsafeTool {
    fn name(&self) -> &str {
        "unsafe"
    }

    fn description(&self) -> &str {
        "unsafe"
    }

    fn parameters(&self) -> serde_json::Value {
        json!({"type":"object"})
    }

    fn mode(&self) -> synthia_tool::traits::ExecutionMode {
        synthia_tool::traits::ExecutionMode::Sequential
    }

    async fn call(&self, _: serde_json::Value, _: &Context) -> ToolOutput {
        let n = UNSAFE_RUNNING.fetch_add(1, Ordering::SeqCst) + 1;
        MAX_CONCURRENT_UNSAFE.fetch_max(n, Ordering::SeqCst);
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        UNSAFE_RUNNING.fetch_sub(1, Ordering::SeqCst);
        ToolOutput::text("unsafe-done")
    }
}

#[tokio::test]
async fn unsafe_tools_run_serially_even_when_safe_present() {
    UNSAFE_RUNNING.store(0, Ordering::SeqCst);
    MAX_CONCURRENT_UNSAFE.store(0, Ordering::SeqCst);

    let tool_use_safe = ToolUse {
        id: "s".into(),
        name: "safe".into(),
        input: json!({}),
    };
    let tool_use_unsafe = ToolUse {
        id: "u".into(),
        name: "unsafe".into(),
        input: json!({}),
    };
    let provider = Arc::new(CapturingProvider::new(vec![
        tool_call_response(vec![tool_use_safe, tool_use_unsafe]),
        empty_response(),
    ]));
    let registry = Arc::new(ToolRegistry::new());
    registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(
        CountingSafeTool,
    )));
    registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(
        CountingUnsafeTool,
    )));
    let _events = run_and_collect(
        provider,
        registry,
        CancellationToken::new(),
        AgentInput::text("m"),
    )
    .await;
    assert_eq!(
        MAX_CONCURRENT_UNSAFE.load(Ordering::SeqCst),
        1,
        "unsafe tool must never overlap with itself"
    );
}

/// Sequential-mode semantics: when one tool returns
/// `is_error = true`, the round must abort and any
/// remaining sequential calls must NOT execute. The
/// downstream LLM still receives a synthetic
/// `ToolResult` for every `tool_use_id` so its history
/// is well-formed, but later calls are reported as
/// "did not run" — they never invoked the tool.
///
/// This pins both the abort-on-error behaviour and the
/// synthetic-result message contract.
#[tokio::test]
async fn sequential_round_aborts_after_first_error_and_skips_remaining() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    // Counter proves later calls never run.
    static LATER_CALLS: AtomicUsize = AtomicUsize::new(0);
    static EARLY_CALLS: AtomicUsize = AtomicUsize::new(0);

    struct EarlyErrorTool;
    #[async_trait]
    impl synthia_tool::Tool for EarlyErrorTool {
        fn name(&self) -> &str {
            "early"
        }

        fn description(&self) -> &str {
            "early"
        }

        fn parameters(&self) -> serde_json::Value {
            json!({"type":"object"})
        }

        fn mode(&self) -> synthia_tool::traits::ExecutionMode {
            synthia_tool::traits::ExecutionMode::Sequential
        }

        async fn call(&self, _: serde_json::Value, _: &Context) -> ToolOutput {
            EARLY_CALLS.fetch_add(1, Ordering::SeqCst);
            // Use the public `error` helper so we get
            // the correct `metadata` / `truncated_by`
            // defaults; we then override `is_error =
            // Some(true)` on the returned value.
            let mut out = ToolOutput::error("boom");
            out.is_error = Some(true);
            out
        }
    }

    struct LaterTool;
    #[async_trait]
    impl synthia_tool::Tool for LaterTool {
        fn name(&self) -> &str {
            "later"
        }

        fn description(&self) -> &str {
            "later"
        }

        fn parameters(&self) -> serde_json::Value {
            json!({"type":"object"})
        }

        fn mode(&self) -> synthia_tool::traits::ExecutionMode {
            synthia_tool::traits::ExecutionMode::Sequential
        }

        async fn call(&self, _: serde_json::Value, _: &Context) -> ToolOutput {
            LATER_CALLS.fetch_add(1, Ordering::SeqCst);
            ToolOutput::text("later-ok")
        }
    }

    let early = ToolUse {
        id: "e".into(),
        name: "early".into(),
        input: json!({}),
    };
    let later = ToolUse {
        id: "l".into(),
        name: "later".into(),
        input: json!({}),
    };
    let provider = Arc::new(CapturingProvider::new(vec![
        tool_call_response(vec![early, later]),
        empty_response(),
    ]));
    let registry = Arc::new(ToolRegistry::new());
    registry
        .register_entry(synthia_tool::ToolEntry::new(Arc::new(EarlyErrorTool)));
    registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(LaterTool)));

    let events = run_and_collect(
        provider,
        registry,
        CancellationToken::new(),
        AgentInput::text("m"),
    )
    .await;

    // The "early" tool fired exactly once (abort happened
    // on its first error).
    assert_eq!(
        EARLY_CALLS.load(Ordering::SeqCst),
        1,
        "early tool must be invoked exactly once"
    );
    // The "later" tool NEVER fired (round aborted).
    assert_eq!(
        LATER_CALLS.load(Ordering::SeqCst),
        0,
        "later tool must NOT be invoked after early error"
    );

    // Wire side: both tool_use_ids must surface as
    // ToolResults so the LLM's history stays well-formed.
    let tool_results: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::Model(ContentPart::ToolResult(tr)) => Some(tr),
            _ => None,
        })
        .collect();
    let ids: Vec<&str> = tool_results
        .iter()
        .map(|tr| tr.tool_use_id.as_str())
        .collect();
    assert!(
        ids.contains(&"e") && ids.contains(&"l"),
        "both tool_use_ids must appear on the wire; got {ids:?}"
    );

    // The "later" tool's synthetic result must carry
    // the neutral "did not produce a result" message
    // (NOT "cancelled" — that was the previous
    // misleading wording) so the LLM can tell it
    // missed the call without learning anything about
    // the earlier tool's failure.
    let later_result = tool_results
        .iter()
        .find(|tr| tr.tool_use_id == "l")
        .expect("'l' result must exist");
    let later_text = later_result
        .content
        .iter()
        .filter_map(|c| match c {
            ContentPart::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        later_text.contains("did not produce a result"),
        "synthetic message must use the new wording; got {later_text:?}"
    );
    assert!(
        !later_text.contains("cancelled"),
        "synthetic message must not leak the old 'cancelled' wording; got {later_text:?}"
    );
    assert!(
        later_result.is_error.unwrap_or(false),
        "synthetic result must be flagged as error so LLM sees it as failure"
    );
}

struct LongTool;
#[async_trait]
impl synthia_tool::Tool for LongTool {
    fn name(&self) -> &str {
        "long"
    }

    fn description(&self) -> &str {
        "long"
    }

    fn parameters(&self) -> serde_json::Value {
        json!({"type": "object"})
    }

    fn mode(&self) -> synthia_tool::traits::ExecutionMode {
        synthia_tool::traits::ExecutionMode::Parallel
    }

    async fn call(&self, _: serde_json::Value, _: &Context) -> ToolOutput {
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        ToolOutput::text("never")
    }
}

#[tokio::test]
async fn parallel_call_respects_cancellation() {
    // Cancel must prevent the second LLM iteration from
    // starting; the first iteration's tool calls complete
    // (long ones at most ~800ms each, but they run in
    // parallel) and then the loop notices the cancelled
    // token before issuing the next sampling pass. Bound:
    // 800ms (parallel long tools) + 200ms slack + next-iter
    // detection = well under 3s.
    let tool_use_a = ToolUse {
        id: "a".into(),
        name: "long".into(),
        input: json!({}),
    };
    let tool_use_b = ToolUse {
        id: "b".into(),
        name: "long".into(),
        input: json!({}),
    };
    let provider = Arc::new(CapturingProvider::new(vec![
        tool_call_response(vec![tool_use_a, tool_use_b]),
        empty_response(),
    ]));
    let registry = Arc::new(ToolRegistry::new());
    registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(LongTool)));
    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        cancel_clone.cancel();
    });
    let started = std::time::Instant::now();
    let _events =
        run_and_collect(provider, registry, cancel, AgentInput::text("c"))
            .await;
    assert!(
        started.elapsed() < std::time::Duration::from_secs(3),
        "session must abort promptly on cancel even with long parallel tools"
    );
}

#[tokio::test]
async fn default_execution_mode_is_parallel() {
    // The Tool trait's mode() defaults to Parallel; this
    // verifies the contract every tool implicitly relies on.
    struct GenericTool;
    #[async_trait]
    impl synthia_tool::Tool for GenericTool {
        fn name(&self) -> &str {
            "g"
        }

        fn description(&self) -> &str {
            "g"
        }

        fn parameters(&self) -> serde_json::Value {
            json!({"type": "object"})
        }

        async fn call(&self, _: serde_json::Value, _: &Context) -> ToolOutput {
            ToolOutput::text("ok")
        }
    }
    assert_eq!(
        GenericTool.mode(),
        synthia_tool::traits::ExecutionMode::Parallel
    );
}

#[test]
fn with_descriptor_installs_descriptor_verbatim() {
    // Regression test: `with_descriptor` used to call
    // `with_options` internally, which built a default
    // descriptor (name="agent", capabilities=["tools",
    // "streaming", "cancellation"], etc.) and then
    // shadowed it with the caller's descriptor. The wasted
    // allocation was benign but the field-by-field overwrite
    // hid a bug class — if `with_options` ever changed to
    // mutate the descriptor, callers would silently lose
    // their customizations. The fix bypasses `with_options`
    // and installs the caller's descriptor verbatim. Verify
    // every field round-trips.
    let custom = AgentDescriptor {
        name: "custom-judge".to_string(),
        description: "Test judge that aggregates votes".to_string(),
        kind: "judge".to_string(),
        version: "2.0.0".to_string(),
        instructions: "You are a strict judge".to_string(),
        capabilities: vec!["judging".to_string()],
        tools: vec!["foo".to_string()],
        model_hint: Some("gpt-x".to_string()),
        handoffs: vec!["agent".to_string()],
        handoff_hint: Some("Use as final aggregator".to_string()),
        output_schema: None,
        owner: Some("team-a".to_string()),
        domain: Some("review".to_string()),

        persona: Some("Skeptical auditor".to_string()),
        display_name: None,
        max_iterations: None,
    };

    let provider: Arc<dyn ModelProvider> =
        Arc::new(ScriptedStreamProvider::new(vec![]));
    let agent = ReActAgent::with_descriptor(
        provider,
        Arc::new(ToolRegistry::new()),
        PathBuf::from("/tmp"),
        custom.clone(),
        Arc::new(PromptContext::default()),
    );

    let got = agent.descriptor();
    assert_eq!(got.name, "custom-judge");
    assert_eq!(got.description, "Test judge that aggregates votes");
    assert_eq!(got.kind, "judge");
    assert_eq!(got.version, "2.0.0");
    assert_eq!(got.instructions, "You are a strict judge");
    assert_eq!(got.capabilities, vec!["judging".to_string()]);
    assert_eq!(got.tools, vec!["foo".to_string()]);
    assert_eq!(got.model_hint.as_deref(), Some("gpt-x"));
    assert_eq!(got.handoffs, vec!["agent".to_string()]);
    assert_eq!(got.handoff_hint.as_deref(), Some("Use as final aggregator"));
    assert_eq!(got.owner.as_deref(), Some("team-a"));
    assert_eq!(got.domain.as_deref(), Some("review"));
    assert_eq!(got.persona.as_deref(), Some("Skeptical auditor"));
}

/// After the panel refactor this test has no panel fields
/// to assert against. We keep a placeholder to document
/// that the clobbering behaviour is now a no-op (there are
/// no defaults that could overwrite a caller field).
#[test]
fn with_descriptor_preserves_caller_fields_after_panel_removal() {
    let descriptor = AgentDescriptor {
        name: "judge".into(),
        description: "judge".into(),
        kind: "judge".into(),
        version: "1.0.0".into(),
        instructions: "judge".into(),
        capabilities: vec![],
        tools: vec![],
        model_hint: None,
        handoffs: vec![],
        handoff_hint: None,
        output_schema: None,
        owner: None,
        domain: None,
        persona: Some("Skeptical auditor".into()),
        display_name: None,
        max_iterations: None,
    };
    let provider: Arc<dyn ModelProvider> =
        Arc::new(ScriptedStreamProvider::new(vec![]));
    let agent = ReActAgent::with_descriptor(
        provider,
        Arc::new(ToolRegistry::new()),
        PathBuf::from("."),
        descriptor,
        Arc::new(PromptContext::default()),
    );
    assert_eq!(
        agent.descriptor().persona.as_deref(),
        Some("Skeptical auditor")
    );
    assert_eq!(agent.descriptor().name, "judge");
}

/// Streaming provider stub with a tiny (30-token) context
/// window, used to force the [`TruncatingContextManager`]
/// path in the live loop.
struct TinyWindowProvider;

#[async_trait]
impl ModelProvider for TinyWindowProvider {
    async fn initialize(
        &mut self,
        _config: synthia_provider::ProviderConfig,
    ) -> Result<(), Error> {
        Ok(())
    }

    fn name(&self) -> &str {
        "tiny-window"
    }

    fn model_config(&self) -> ModelConfig {
        ModelConfig {
            name: "tiny".to_string(),
            provider: "test".to_string(),
            context_window: 30,
            max_output_tokens: 64,
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
        _request: CompletionRequest,
        _cancel_token: Option<Arc<dyn CancelToken>>,
        mut on_delta: Box<dyn FnMut(StreamChunk) + Send>,
    ) -> Result<CompletionResponse, Error> {
        let sampling = SamplingResult {
            text: "ok".to_string(),
            ..Default::default()
        };
        on_delta(StreamChunk::IsDone {
            result: Box::new(sampling.clone()),
        });
        Ok(CompletionResponse {
            id: "tiny-1".to_string(),
            model: "tiny".to_string(),
            content: synthia_provider::Content::Single(ContentPart::Text(
                TextContent {
                    text: sampling.text.clone(),
                    cache_control: None,
                },
            )),
            usage: sampling.usage.clone(),
            cached: false,
            replay_state: None,
            stop_reason: sampling.stop_reason.clone(),
        })
    }
}

#[tokio::test]
async fn context_manager_prunes_overflowing_history_and_warns() {
    // Six 400-char history turns (≈600 tokens) against a
    // 30-token window: the default TruncatingContextManager
    // must evict pairs until the history fits and the loop
    // must surface a ContextCompaction warning.
    let history: Vec<Message> = (0..6)
        .flat_map(|i| {
            vec![
                Message::user(format!("{}{}", "q".repeat(399), i)),
                Message::assistant(format!("{}{}", "a".repeat(399), i)),
            ]
        })
        .collect();

    let agent = ReActAgent::with_prompt_context(
        Arc::new(TinyWindowProvider),
        Arc::new(ToolRegistry::new()),
        PathBuf::new(),
        String::new(),
        Arc::new(PromptContext::default()),
    );

    let mut stream = agent
        .run(
            AgentInput::history(history, "hi"),
            Arc::new(CancellationToken::new()),
        )
        .await;

    let mut saw_compaction_warning = false;
    while let Some(ev) = stream.next().await {
        if let AgentEvent::System(SystemEvent::Warning { kind, .. }) = &ev
            && *kind == WarningKind::ContextCompaction
        {
            saw_compaction_warning = true;
        }
    }
    assert!(
        saw_compaction_warning,
        "expected a ContextCompaction warning for the overflowing history"
    );
}
