//! # The minimal Synthia agent, proven by compiling it
//!
//! The executable form of [`MINIMAL.md`](../../../MINIMAL.md): **one**
//! dependency, the seven-piece feature subset the guide names, and a
//! full turn that calls a tool and returns an answer. If this crate
//! stops building or the run stops working, the "you can start with
//! almost nothing" claim in the docs is false.
//!
//! Run it:
//!
//! ```bash
//! cd docs/examples/minimal-consumer && cargo run
//! ```
//!
//! Expected tail: `MVP-OK`
//!
//! What is deliberately **absent** here — and what a consumer who does
//! not need it never pays for:
//!
//! | Absent | Why it is not needed for an MVP |
//! |---|---|
//! | `synthia-macros` (`#[derive(Tool)]`) | one hand-written `Tool` impl is four small methods |
//! | `synthia-mcp` / `synthia-skill` | remote tools and skills are opt-in extensions |
//! | `synthia-scheduler` / `synthia-workflow` / `synthia-eval` | cron, workflows and eval suites are separate products |
//! | `sqlite` | memory persistence is a storage decision the consumer makes |
//! | `synthia-server` / `synthia-telemetry` | an HTTP deployment and an OTLP pipeline are deployment choices |
//!
//! Every remaining dependency is a `synthia-*` crate or a wire type
//! (`futures`, `serde_json`); `tokio` is the consumer's own choice of
//! executor for `main`, not a framework requirement.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use futures::StreamExt;
use synthia::prelude::*;

// ---------------------------------------------------------------------
// 1. The provider — the only piece with no default.
//
// Two scripted calls: the first asks for the `echo` tool, the second
// answers. That is enough to exercise the loop, the tool registry, the
// context manager and the session sink in one turn.
// ---------------------------------------------------------------------
struct ScriptedProvider {
    calls: Arc<AtomicUsize>,
}

impl ScriptedProvider {
    fn new(calls: Arc<AtomicUsize>) -> Self {
        Self { calls }
    }
}

#[async_trait]
impl ModelProvider for ScriptedProvider {
    async fn initialize(
        &mut self,
        _config: ProviderConfig,
    ) -> Result<(), Error> {
        Ok(())
    }

    fn name(&self) -> &str {
        "scripted"
    }

    fn model_config(&self) -> ModelConfig {
        ModelConfig {
            name: "scripted-1".to_string(),
            provider: "scripted".to_string(),
            context_window: 8_192,
            max_output_tokens: 1_024,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: false,
        }
    }

    // `embed` is *not* implemented: it has a default impl that returns
    // an empty vector per text. Providers that ship an embedding
    // endpoint override it; adapters that don't drive embeddings leave
    // it alone.
    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> Result<CompletionResponse, Error> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        let content = if call == 0 {
            Content::parts(vec![ContentPart::ToolUse(synthia::provider::ToolUse {
                id: "call-1".to_string(),
                name: "echo".to_string(),
                input: serde_json::json!({ "text": "minimal" }),
            })])
        } else {
            Content::text("the echo tool ran, so the loop works")
        };
        Ok(CompletionResponse {
            content,
            ..CompletionResponse::default()
        })
    }
}

// ---------------------------------------------------------------------
// 2. The tools — no agent-facing set ships; the consumer composes.
// ---------------------------------------------------------------------
struct EchoTool {
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl Tool for EchoTool {
    fn name(&self) -> &str {
        "echo"
    }

    fn description(&self) -> &str {
        "Echo the `text` argument back."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": { "text": { "type": "string" } },
            "required": ["text"],
        })
    }

    async fn call(
        &self,
        input: serde_json::Value,
        _context: &Context,
    ) -> ToolOutput {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let text = input
            .get("text")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("(missing `text`)");
        ToolOutput::text(format!("echo: {text}"))
    }
}

#[tokio::main]
async fn main() {
    println!("=== synthia: the minimal agent (7 features, 1 dependency) ===\n");

    let model_calls = Arc::new(AtomicUsize::new(0));
    let tool_calls = Arc::new(AtomicUsize::new(0));

    // 1. provider
    let provider: Arc<dyn ModelProvider> =
        Arc::new(ScriptedProvider::new(Arc::clone(&model_calls)));

    // 2. tools
    // The `tool` feature is the paradigm (registry + trait), not a
    // tool set: each builtin lives in its own plugin crate behind
    // its own feature, so this MVP subset compiles **zero** tool
    // implementations. This agent registers exactly the one tool it
    // hand-wrote. (That no network-capable tool can exist in the
    // binary is proven by the facade's `compile_fail` doc test for
    // `tool-web` — live in this feature configuration — and by
    // `make check-mvp-deps`.)
    let registry = ToolRegistry::new();
    registry.register_entry(synthia::tool::ToolEntry::new(Arc::new(EchoTool {
        calls: Arc::clone(&tool_calls),
    })));
    // The registry holds the hand-written tool and nothing else: if
    // the paradigm crate ever started seeding a default tool set
    // again (the coupling this split removed), this fails.
    assert_eq!(
        registry.tool_count(),
        1,
        "ToolRegistry::new() must seed nothing; only the hand-written tool is registered"
    );
    let tool_count = registry.tool_count();

    // 3. steering — empty on purpose: guards and hints are opt-in.
    let steering = Arc::new(Steering::noop());

    // 4. context manager — drop the oldest pairs until the window fits.
    let context_manager: Arc<dyn ContextManager> =
        Arc::new(TruncatingContextManager);

    // 5. session sink — a bounded channel the consumer drains.
    let (typed_sink, mut typed_rx) = TypedEventSink::channel(64);

    // 6. cancel token — std only, no runtime type in the signature.
    let cancel: Arc<dyn CancelToken> = AtomicCancelToken::shared();

    // 7. the agent
    let agent = ReActAgent::new(provider, Arc::new(registry))
        .with_workspace(".")
        .with_steering(steering)
        .with_context_manager(context_manager)
        .with_typed_event_sink(typed_sink)
        .with_max_iterations(4)
        .with_name("minimal-consumer")
        .with_instructions("Answer in one short sentence.");

    let mut stream = agent
        .run(AgentInput::text("Use the echo tool, then answer."), cancel)
        .await;

    let mut answer = String::new();
    while let Some(event) = stream.next().await {
        match &event {
            AgentEvent::Model(part) => {
                if let Some(text) = part.text() {
                    answer.push_str(text);
                }
            }
            AgentEvent::ModelDone(done) => answer = done.text.clone(),
            AgentEvent::System(_) | AgentEvent::Agent(_, _) => {}
        }
    }

    let mut structural = 0usize;
    while let Ok(Some(record)) = typed_rx.try_recv() {
        let _ = record.as_value();
        structural += 1;
    }

    assert_eq!(
        tool_calls.load(Ordering::SeqCst),
        1,
        "the loop must execute the tool the model asked for"
    );
    assert_eq!(
        model_calls.load(Ordering::SeqCst),
        2,
        "one tool round-trip means two model calls"
    );
    assert!(
        answer.contains("the loop works"),
        "the provider's final answer must reach the caller, got {answer:?}"
    );

    println!("model calls      : {}", model_calls.load(Ordering::SeqCst));
    println!("tool calls       : {}", tool_calls.load(Ordering::SeqCst));
    println!("structural events: {structural}");
    println!(
        "tool registry    : {tool_count} tool (hand-written; no agent-facing set ships)"
    );
    println!("answer           : {answer}");
    println!("\nMVP-OK");
}
