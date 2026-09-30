//! # Swap the reasoning loop, keep everything else
//!
//! Run it:
//!
//! ```text
//! cargo run --example strategy_swap -p synthia-harness
//! ```
//!
//! The same provider, the same tool registry, the same `ReActAgent` —
//! three different reasoning paradigms:
//!
//! | Run | Strategy | Requests | Offers tools | Tool calls |
//! |---|---|---|---|---|
//! | 1 | `ReActStrategy` (default) | 2 | yes | 1 (`echo`) |
//! | 2 | `ChainOfThoughtStrategy` | 1 | no | 0 |
//! | 3 | `BestOfNStrategy` | 3 | no | 0 |
//!
//! That is the whole point of the strategy seam: the *loop* is a
//! variable, the plumbing is not. A strategy receives an
//! `AgentRuntime` holding every assembled piece and publishes events
//! through an `EventSink`; the agent is a three-line adapter over it.
//!
//! Run 3 is the interesting one structurally: `BestOfNStrategy` fans
//! three samples out through `AgentRuntime::spawner` (the deployment's
//! executor — this example runs on tokio, a consumer could supply its
//! own), scores each with a [`CandidateScorer`] the caller supplies,
//! and publishes only the winner. Alternatives stay off-stream.
//!
//! It ends by printing `STRATEGY-SWAP: OK` after asserting — from the
//! provider's own recorded requests — that ReAct advertised tools and
//! used one, Chain-of-Thought advertised none and used none, and
//! Best-of-N sampled three times and selected the candidate its scorer
//! preferred.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use futures::StreamExt;
use parking_lot::Mutex;
use synthia_core::{AtomicCancelToken, CancelToken};
use synthia_harness::{
    Agent,
    AgentEvent,
    AgentInput,
    ReActAgent,
    agent::{
        BestOfNStrategy,
        CandidateScorer,
        ChainOfThoughtStrategy,
        ReActStrategy,
    },
};
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

/// Answers from a script and remembers every request — so the example
/// can *prove* what each strategy asked for, not just what it printed.
struct ScriptedProvider {
    script: Mutex<Vec<Content>>,
    requests: Mutex<Vec<CompletionRequest>>,
}

impl ScriptedProvider {
    fn new(script: Vec<Content>) -> Arc<Self> {
        Arc::new(Self {
            script: Mutex::new(script),
            requests: Mutex::new(Vec::new()),
        })
    }

    fn requests(&self) -> Vec<CompletionRequest> {
        self.requests.lock().clone()
    }
}

#[async_trait::async_trait]
impl ModelProvider for ScriptedProvider {
    async fn initialize(
        &mut self,
        _config: ProviderConfig,
    ) -> Result<(), synthia_core::Error> {
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
        request: CompletionRequest,
    ) -> Result<CompletionResponse, synthia_core::Error> {
        self.requests.lock().push(request);
        let content = {
            let mut script = self.script.lock();
            if script.is_empty() {
                Content::text("(script exhausted)")
            } else {
                script.remove(0)
            }
        };
        Ok(CompletionResponse {
            content,
            ..CompletionResponse::default()
        })
    }
}

/// Counts its own invocations — the observable difference between the
/// two runs.
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

/// Drain one run, printing what happened.
async fn run_and_print(agent: &dyn Agent, prompt: &str) -> String {
    let cancel: Arc<dyn CancelToken> = AtomicCancelToken::shared();
    let mut stream = agent.run(AgentInput::text(prompt), cancel).await;

    let mut answer = String::new();
    let mut events = 0usize;
    while let Some(event) = stream.next().await {
        events += 1;
        match &event {
            AgentEvent::ModelDone(done) => answer = done.text.clone(),
            AgentEvent::System(
                synthia_harness::events::SystemEvent::SessionEnded { reason },
            ) => {
                println!("      ended        : {reason:?}");
            }
            AgentEvent::System(
                synthia_harness::events::SystemEvent::Progress {
                    message, ..
                },
            ) => println!("      progress     : {message}"),
            _ => {}
        }
    }
    println!("      events       : {events}");
    println!("      answer       : {}", answer.replace('\n', " ⏎ "));
    answer
}

#[tokio::main]
async fn main() {
    println!("=== synthia: one runtime, three reasoning loops ===\n");

    let tool_calls = Arc::new(AtomicUsize::new(0));

    // ---- run 1: ReAct (the default) ---------------------------------
    println!("[1/3] ReActStrategy — tools are offered, the loop uses them");
    let react_provider = ScriptedProvider::new(vec![
        // First pass: ask for the tool.
        Content::parts(vec![ContentPart::ToolUse(ToolUse {
            id: "call-1".to_string(),
            name: "echo".to_string(),
            input: serde_json::json!({ "text": "react" }),
        })]),
        // Second pass: answer.
        Content::text("react answered after echoing"),
    ]);
    let react = ReActAgent::new(
        react_provider.clone(),
        Arc::new(registry_with_echo(Arc::clone(&tool_calls))),
    )
    .with_workspace(".")
    .with_strategy(Arc::new(ReActStrategy))
    .with_max_iterations(4)
    .with_name("react-run");
    println!("      strategy     : {}", react.strategy().name());
    let react_answer = run_and_print(&react, "Use echo, then answer.").await;
    let react_tool_calls = tool_calls.load(Ordering::SeqCst);
    let react_requests = react_provider.requests();
    println!(
        "      tools offered: {} (first request)",
        react_requests[0].tools.len()
    );
    println!("      tool calls   : {react_tool_calls}");

    // ---- run 2: Chain-of-Thought, same builder, same registry -------
    println!("\n[2/3] ChainOfThoughtStrategy — no tools, step-by-step prompt");
    let cot_provider = ScriptedProvider::new(vec![Content::text(
        "Step 1: read the request.\nStep 2: answer directly.\nAnswer: cot answered without tools",
    )]);

    let cot = ReActAgent::new(
        cot_provider.clone(),
        Arc::new(registry_with_echo(Arc::clone(&tool_calls))),
    )
    .with_workspace(".")
    .with_strategy(Arc::new(ChainOfThoughtStrategy::new(3)))
    .with_max_iterations(4)
    .with_name("cot-run");
    println!("      strategy     : {}", cot.strategy().name());
    let cot_answer = run_and_print(&cot, "What is 2 + 2?").await;
    let cot_requests = cot_provider.requests();
    println!(
        "      tools offered: {} (first request)",
        cot_requests[0].tools.len()
    );
    println!(
        "      steps parsed : {:?}",
        ChainOfThoughtStrategy::parse_steps(&cot_answer)
    );

    // ---- run 3: Best-of-N, same builder, same registry ---------------
    println!("\n[3/3] BestOfNStrategy — three samples, one winner, no tools");

    /// Scores the candidate that carries the marker the caller needs.
    /// In production this is a verifier, a test run, or an LLM judge —
    /// the strategy does not care which.
    struct RequiresMarker;

    #[async_trait::async_trait]
    impl CandidateScorer for RequiresMarker {
        async fn score(&self, candidate: &str) -> f64 {
            if candidate.contains("REQUIRED") {
                1.0
            } else {
                0.0
            }
        }

        fn name(&self) -> &str {
            "requires-marker"
        }
    }

    // Deliberately unordered by quality: the marker is in the middle
    // sample, and the samples run concurrently, so nothing but the
    // scorer can be choosing the winner.
    let bon_provider = ScriptedProvider::new(vec![
        Content::text("a plain answer"),
        Content::text("the REQUIRED answer"),
        Content::text("another plain answer, somewhat longer"),
    ]);
    let bon = ReActAgent::new(
        bon_provider.clone(),
        Arc::new(registry_with_echo(Arc::clone(&tool_calls))),
    )
    .with_workspace(".")
    .with_strategy(Arc::new(BestOfNStrategy::new(3, Arc::new(RequiresMarker))))
    .with_max_iterations(4)
    .with_name("bon-run");
    println!("      strategy     : {}", bon.strategy().name());
    let bon_answer = run_and_print(&bon, "Answer with the marker.").await;
    let bon_requests = bon_provider.requests();
    println!("      samples      : {}", bon_requests.len());
    println!(
        "      tools offered: {} (every sample)",
        bon_requests
            .iter()
            .map(|r| r.tools.len())
            .max()
            .unwrap_or_default()
    );

    // ---- the proof ---------------------------------------------------
    assert!(
        !react_requests[0].tools.is_empty(),
        "ReAct must advertise the registry's tools"
    );
    assert_eq!(
        react_tool_calls, 1,
        "ReAct must actually run the tool the model asked for"
    );
    assert!(
        cot_requests[0].tools.is_empty(),
        "chain-of-thought must not advertise tools"
    );
    assert_eq!(
        cot_requests[0].messages.len(),
        2,
        "one system prompt + the user turn"
    );
    assert!(
        cot_requests[0].messages[0]
            .content
            .extract_text()
            .is_some_and(
                |text| text.contains("Step 1:") || text.contains("Step N")
            ),
        "the CoT instruction must reach the system prompt"
    );
    assert_eq!(
        tool_calls.load(Ordering::SeqCst),
        1,
        "the second run must not have run the tool"
    );
    assert!(react_answer.contains("react answered"));
    assert!(cot_answer.contains("cot answered without tools"));
    assert_eq!(
        bon_requests.len(),
        3,
        "best-of-n must sample once per candidate"
    );
    assert!(
        bon_requests.iter().all(|r| r.tools.is_empty()),
        "candidates must be comparable answers, not tool-using loops"
    );
    assert!(
        bon_answer.contains("REQUIRED"),
        "the scorer's preference must decide the winner: {bon_answer}"
    );
    assert_eq!(
        tool_calls.load(Ordering::SeqCst),
        1,
        "no run after the first may execute a tool"
    );

    println!("\nSTRATEGY-SWAP: OK");
}

fn registry_with_echo(calls: Arc<AtomicUsize>) -> ToolRegistry {
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(EchoTool { calls })));
    registry
}
