//! # Assemble an agent from zero — through the facade
//!
//! Run it:
//!
//! ```text
//! cargo run -p synthia --example assemble_from_zero
//! ```
//!
//! The executable form of the crate docs' tutorial. Everything comes
//! from the facade — the assembly itself through
//! [`synthia::prelude`], plus the plugin tool crates
//! (`synthia::tool_read`, …) composed brick by brick — and the
//! program needs no network, no API key, and no environment
//! variable: the model is a scripted [`ModelProvider`] defined
//! below.
//!
//! The seven pieces, in the order a consumer meets them:
//!
//! | # | Piece | Type |
//! |---|---|---|
//! | 1 | the model | [`ModelProvider`] |
//! | 2 | the tools | [`ToolRegistry`] + plugin bricks + one hand-written [`Tool`] |
//! | 3 | the steering | [`Steering`] |
//! | 4 | the context manager | [`ContextManager`] |
//! | 5 | the session sink | [`TypedEventSink`] |
//! | 6 | the cancel token | [`CancelToken`] |
//! | 7 | the agent | [`ReActAgent`] |
//!
//! It ends by printing `ASSEMBLE-FROM-ZERO: OK` once the assembled
//! agent has run a full turn.

use std::sync::Arc;

use futures::StreamExt;
use synthia::prelude::*;

// ---------------------------------------------------------------------
// 1. The provider: the one piece the framework cannot supply.
//
// A real provider talks to a model and overrides
// `complete_with_stream` to emit incremental `StreamChunk`s; this one
// implements only `complete`, so the default streaming impl wraps it
// in a single terminal `IsDone` chunk.
// ---------------------------------------------------------------------
struct ScriptedProvider {
    answer: String,
}

impl ScriptedProvider {
    fn new(answer: impl Into<String>) -> Self {
        Self {
            answer: answer.into(),
        }
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

    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> Result<CompletionResponse, Error> {
        Ok(CompletionResponse {
            content: Content::text(self.answer.clone()),
            ..CompletionResponse::default()
        })
    }
}

// ---------------------------------------------------------------------
// 2. The tools: a `Tool` is three descriptions plus one async call.
// No agent-facing set ships with the framework — `main` composes the
// registry from the plugin crates, and `EchoTool` is the extension
// point a consumer actually writes.
// ---------------------------------------------------------------------
struct EchoTool;

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
        let text = input
            .get("text")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("(missing `text`)");
        ToolOutput::text(format!("echo: {text}"))
    }
}

#[tokio::main]
async fn main() {
    println!("=== synthia: assemble an agent from zero (prelude only) ===\n");

    // --- 1. provider ------------------------------------------------
    let provider: Arc<dyn ModelProvider> =
        Arc::new(ScriptedProvider::new("I was assembled from seven pieces."));
    println!("[1/7] provider       : {}", provider.name());
    println!(
        "                     : window={} max_output={} tools={}",
        provider.model_config().context_window,
        provider.model_config().max_output_tokens,
        provider.model_config().supports_tools,
    );

    // --- 2. tools ---------------------------------------------------
    // The paradigm ships no agent-facing set: each plugin crate is
    // one line, and the hand-written `EchoTool` below is the
    // extension point a consumer actually writes.
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(
        synthia::tool_read::ReadTool::new(),
    )));
    registry.register_entry(ToolEntry::new(Arc::new(
        synthia::tool_write::WriteTool::new(),
    )));
    registry.register_entry(ToolEntry::new(Arc::new(
        synthia::tool_shell::ShellTool::new(),
    )));
    registry.register_entry(ToolEntry::new(Arc::new(
        synthia::tool_todo::TodoWriteTool::new(),
    )));
    let plugins = registry.tool_count();
    registry.register_entry(ToolEntry::new(Arc::new(EchoTool)));
    println!(
        "[2/7] tools          : {plugins} plugin builtins + echo = {} total",
        registry.tool_count()
    );

    // The tool contract also runs standalone: JSON in, `ToolOutput`
    // out, no registry and no loop involved.
    let probe = EchoTool
        .call(
            serde_json::json!({ "text": "hello" }),
            &Context::new("assemble-from-zero".to_string(), ".".into()),
        )
        .await;
    println!("                     : echo tool says {probe:?}");

    // --- 3. steering ------------------------------------------------
    let steering = Arc::new(Steering::noop());
    println!("[3/7] steering       : noop (no guards, hints, or hooks)");

    // --- 4. context manager -----------------------------------------
    let context_manager: Arc<dyn ContextManager> =
        Arc::new(TruncatingContextManager);
    println!("[4/7] context mgr    : TruncatingContextManager");

    // --- 5. session sink --------------------------------------------
    let (typed_sink, mut typed_rx) = TypedEventSink::channel(64);
    println!("[5/7] typed channel  : capacity 64 (futures mpsc)");

    // --- 7. the agent ------------------------------------------------
    let agent = ReActAgent::new(provider, Arc::new(registry))
        .with_workspace(".")
        .with_steering(steering)
        .with_context_manager(context_manager)
        .with_typed_event_sink(typed_sink)
        .with_max_iterations(4)
        .with_name("assemble-from-zero")
        .with_instructions("Answer in one short sentence.");
    println!(
        "[7/7] agent          : {} (max {} iterations)",
        agent.descriptor().name,
        agent.effective_max_iterations(),
    );

    // --- 6. cancel token --------------------------------------------
    // std-only: the agent's public contract names no runtime.
    let cancel: Arc<dyn CancelToken> = AtomicCancelToken::shared();
    println!(
        "[6/7] cancel token   : AtomicCancelToken, cancelled={}\n",
        cancel.is_cancelled()
    );

    // --- run one turn -------------------------------------------------
    println!("--- run ---");
    let mut stream = agent
        .run(AgentInput::text("Introduce yourself."), cancel)
        .await;

    let mut answer = String::new();
    let mut lifecycle: Vec<&'static str> = Vec::new();
    while let Some(event) = stream.next().await {
        match &event {
            AgentEvent::Model(part) => {
                if let Some(text) = part.text() {
                    answer.push_str(text);
                }
            }
            AgentEvent::ModelDone(done) => {
                println!("      answer       : {}", done.text.trim());
                println!("      tool calls   : {}", done.tool_calls.len());
                answer = done.text.clone();
            }
            AgentEvent::System(sys) => lifecycle.push(sys.kind()),
            AgentEvent::Agent(_, _) => {}
        }
    }
    println!("      lifecycle    : {lifecycle:?}");

    // The structural events the loop published to the typed sink —
    // drained synchronously, because every record is buffered before
    // the run's event stream ends.
    let mut structural = 0usize;
    while let Ok(Some(record)) = typed_rx.try_recv() {
        println!("      typed event  : {}", record.as_value());
        structural += 1;
    }
    println!("      typed events : {structural}");

    assert!(
        answer.contains("assembled from seven pieces"),
        "the scripted provider's answer must reach the caller, got {answer:?}"
    );
    assert!(
        lifecycle.contains(&"SessionEnded"),
        "a run must end with a terminal event, got {lifecycle:?}"
    );

    println!("\nASSEMBLE-FROM-ZERO: OK");
}
