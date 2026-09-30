//! # Run the agent loop on an executor that is not tokio
//!
//! ```bash
//! cargo run -p synthia-harness --example runtime_agnostic
//! ```
//!
//! Expected tail: `RUNTIME-AGNOSTIC: OK`
//!
//! The framework's lego promise includes the executor: a consumer who
//! did not choose tokio should still be able to assemble an agent.
//! [`Agent::run`] detaches one task per run (so a caller that stops
//! polling the event stream does not cancel the run), and that detach
//! goes through [`synthia_core::spawn::Spawner`] — a trait with no
//! runtime in its signature.
//!
//! Nothing in this program links tokio into its own `main`:
//!
//! - the spawner is [`ThreadSpawner`] below — `std::thread::spawn` plus
//!   `futures::executor::block_on`, ten lines;
//! - `main` is a plain `fn main()`, and every await happens inside
//!   `futures::executor::block_on`;
//! - the provider is scripted (no HTTP adapter, no retry timer);
//! - the tools the model may call are the consumer's own `Tool`
//!   implementations — the builtin `read` / `write` / `shell` are
//!   tokio-bound plugins, so this program does not register them.
//!
//! What is *not* runtime-neutral is documented on
//! [`synthia_core::spawn`]: the builtin tools, the provider HTTP
//! adapters, and the JSONL session sink. Everything the loop itself
//! needs — context managers, steering, the event stream, cancellation —
//! is `futures` plus [`Spawner`].

use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
};

use futures::StreamExt;
use synthia_core::{
    AtomicCancelToken,
    CancelToken,
    Error,
    spawn::{BoxFuture, Spawner},
};
use synthia_harness::{Agent, AgentEvent, AgentInput, ReActAgent};
use synthia_provider::{
    CompletionRequest,
    CompletionResponse,
    Content,
    ContentPart,
    ModelConfig,
    ModelProvider,
    ProviderConfig,
    ToolUse,
};
use synthia_tool::{Tool, ToolOutput, ToolRegistry, registry::ToolEntry};

/// The one thing a non-tokio consumer must supply: where detached work
/// goes. This implementation gives every task its own OS thread and
/// drives it with the `futures` block-on executor — the smallest
/// runtime that can run a `Send` future, and definitively not tokio.
struct ThreadSpawner;

impl Spawner for ThreadSpawner {
    fn spawn(&self, task: BoxFuture<()>) {
        thread::spawn(move || futures::executor::block_on(task));
    }

    fn spawn_blocking(&self, f: Box<dyn FnOnce() + Send + 'static>) {
        thread::spawn(f);
    }
}

/// Scripted provider: ask for the `echo` tool once, then answer.
struct ScriptedProvider {
    calls: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
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
            supports_streaming: false,
            supports_reasoning: false,
        }
    }

    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> Result<CompletionResponse, Error> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        let content = if call == 0 {
            Content::parts(vec![ContentPart::ToolUse(ToolUse {
                id: "call-1".to_string(),
                name: "echo".to_string(),
                input: serde_json::json!({ "text": "no tokio here" }),
            })])
        } else {
            Content::text("the loop ran without tokio")
        };
        Ok(CompletionResponse {
            content,
            ..CompletionResponse::default()
        })
    }
}

/// A consumer-owned tool — no filesystem, no process, no timer.
struct EchoTool {
    calls: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
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
        _context: &synthia_tool::Context,
    ) -> ToolOutput {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let text = input
            .get("text")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("(missing)");
        ToolOutput::text(format!("echo: {text}"))
    }
}

fn main() {
    println!("=== synthia: agent loop on a non-tokio executor ===\n");

    let model_calls = Arc::new(AtomicUsize::new(0));
    let tool_calls = Arc::new(AtomicUsize::new(0));
    let spawner: Arc<dyn Spawner> = Arc::new(ThreadSpawner);

    let provider: Arc<dyn ModelProvider> = Arc::new(ScriptedProvider {
        calls: Arc::clone(&model_calls),
    });

    let registry = ToolRegistry::new().with_spawner(Arc::clone(&spawner));
    registry.register_entry(ToolEntry::new(Arc::new(EchoTool {
        calls: Arc::clone(&tool_calls),
    })));

    let agent = ReActAgent::new(provider, Arc::new(registry))
        .with_workspace(".")
        .with_spawner(Arc::clone(&spawner))
        .with_max_iterations(4)
        .with_name("runtime-agnostic")
        .with_instructions("Answer in one short sentence.");

    let cancel: Arc<dyn CancelToken> = AtomicCancelToken::shared();

    // No `#[tokio::main]`: the `futures` executor drives everything.
    let mut stream = futures::executor::block_on(
        agent.run(AgentInput::text("Use the echo tool, then answer."), cancel),
    );

    let events = futures::executor::block_on(async move {
        let mut collected = Vec::new();
        while let Some(event) = stream.next().await {
            collected.push(event);
        }
        collected
    });

    let mut answer = String::new();
    for event in &events {
        if let AgentEvent::ModelDone(done) = event {
            answer.clone_from(&done.text);
        }
    }

    assert_eq!(
        tool_calls.load(Ordering::SeqCst),
        1,
        "the tool must have run on the injected executor"
    );
    assert_eq!(model_calls.load(Ordering::SeqCst), 2);
    assert!(
        answer.contains("without tokio"),
        "the final answer must reach the caller, got {answer:?}"
    );

    println!("spawner          : std::thread + futures::executor::block_on");
    println!("events observed  : {}", events.len());
    println!("model calls      : {}", model_calls.load(Ordering::SeqCst));
    println!("tool calls       : {}", tool_calls.load(Ordering::SeqCst));
    println!("answer           : {answer}");
    println!("\nRUNTIME-AGNOSTIC: OK");
}
