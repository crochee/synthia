//! `assemble_with_builder` — the modern one-expression assembly.
//!
//! R14 capstone tutorial. Every earlier `assemble_*` example
//! wires the bricks by hand (each `with_*` call held by the
//! caller). `ReActAgent::new(provider, registry)` collapses that
//! into one chained expression (R12's `AgentBuilder` folded into
//! the agent itself in R110); R13's `output_schema` and
//! `compaction_settings` setters ride the same chain. This file
//! is the "best-assembly paradigm" the project goal names: a
//! complete, production-shaped agent in one readable block.
//!
//! Demonstrated bricks:
//!
//! | Brick | Round | API |
//! |---|---|---|
//! | `ReActAgent` (the builder is the harness) | R12 / R110 | `ReActAgent::new(provider, registry)` chain |
//! | Tier-aware steering | R10 | `synthia_steering::tier::auto` |
//! | Structured output | R12+R13 | `.with_output_schema(schema)` → auto tool |
//! | LLM compaction | R13 | `.with_compaction_settings(..)` → summarising manager |
//! | Typed event sink | R6 | `.with_typed_event_sink(sink)` |
//! | @agent router bridge | R10+R11 | `LeaderRouter` + `mentions_to_task_specs` |
//!
//! Run (no network, no API key):
//!
//! ```bash
//! cargo run --example assemble_with_builder -p synthia-harness
//! ```

use std::sync::Arc;

use synthia_context::CompactionSettings;
use synthia_harness::{ReActAgent, agent::Agent as _};
use synthia_provider::traits::ModelProvider;
use synthia_session::TypedEventSink;
use synthia_steering::tier::auto as tier_steering;
use synthia_tool::{ToolEntry, ToolRegistry};
use synthia_tool_read::ReadTool;
use synthia_tool_shell::ShellTool;
use synthia_tool_task::{LeaderRouter, Router as _, mentions_to_task_specs};
use synthia_tool_todo::TodoWriteTool;
use synthia_tool_write::WriteTool;

/// The default surface, composed explicitly: one line per plugin
/// crate, exactly the bricks this example wants.
fn plugin_tool_registry() -> Arc<ToolRegistry> {
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(ReadTool::new())));
    registry.register_entry(ToolEntry::new(Arc::new(WriteTool::new())));
    registry.register_entry(ToolEntry::new(Arc::new(ShellTool::new())));
    registry.register_entry(ToolEntry::new(Arc::new(TodoWriteTool::new())));
    Arc::new(registry)
}

#[tokio::main]
async fn main() {
    println!("== synthia R14: assemble with ReActAgent ==\n");

    // 1. Provider — the stub answers locally (no network).
    let provider: Arc<dyn ModelProvider> =
        Arc::new(synthia_provider::traits_stub::ModelProviderStub::text_only(
            "Structured answer captured via the structured_output tool.",
        ));
    println!("[1/6] provider      : {}", provider.name());

    // 2. Tier-aware steering (R10): one line picks the whole
    //    guard/hint/tracker bundle from the provider's tier.
    let workspace = std::path::PathBuf::from(".");
    let steering = Arc::new(tier_steering(provider.tier(), &workspace));
    println!(
        "[2/6] steering      : {} guards / {} hints",
        steering.guards.len(),
        steering.hints.len()
    );

    // 3. Typed event sink (R6): structural events stream to a
    //    futures-mpsc channel (runtime-neutral).
    let (typed_sink, mut typed_rx) = TypedEventSink::channel(64);
    println!("[3/6] typed sink    : channel open");

    // 4-6. THE BUILD: one chained expression consumes the bricks.
    //    - `.output_schema(..)` (R13) auto-registers the
    //      `structured_output` tool AND stamps the descriptor.
    //    - `.compaction_settings(..)` (R13) upgrades the context
    //      manager to the LLM-backed summariser (same provider).
    //    - `.steering(..)` / `.typed_event_sink(..)` / `.name()`
    //      complete the wiring.
    let agent = ReActAgent::new(provider, plugin_tool_registry())
        .with_name("tutorial-agent")
        .with_instructions("Answer through the structured_output tool.")
        .with_workspace(&workspace)
        .with_steering(steering)
        .with_output_schema(serde_json::json!({
            "type": "object",
            "properties": {
                "answer": {"type": "string"},
                "confidence": {"type": "number"}
            },
            "required": ["answer"]
        }))
        .with_compaction_settings(CompactionSettings::default())
        .with_typed_event_sink(typed_sink);

    println!(
        "[4/6] descriptor    : name={} (schema {})",
        agent.descriptor().name,
        if agent.descriptor().output_schema.is_some() {
            "declared"
        } else {
            "none"
        }
    );
    println!(
        "[5/6] structured out: tool auto-injected (output_schema declared)"
    );
    println!("[6/6] compaction    : LLM summariser armed (default settings)");

    // Bonus brick: the @agent router bridge (R10+R11). Text
    // mentions become `TaskSpec`s for the delegation seam.
    let router = LeaderRouter::new(["researcher"].map(String::from));
    let decision = router.route("@researcher: draft the summary.");
    let specs = mentions_to_task_specs(&decision);
    if let Some(spec) = specs.first() {
        println!(
            "\nrouter           : @mention → task(agent='{}', prompt='{}')",
            spec.agent, spec.prompt
        );
    }

    // Drive one turn so the whole assembly is proven live.
    use futures::StreamExt;
    let cancel = Arc::new(synthia_core::AtomicCancelToken::new());
    let mut stream = agent
        .run(
            synthia_harness::AgentInput::text("Give me the structured answer."),
            cancel,
        )
        .await;
    let mut answer = String::new();
    let mut events = 0usize;
    while let Some(event) = stream.next().await {
        events += 1;
        if let synthia_harness::AgentEvent::Model(
            synthia_provider::ContentPart::Text(t),
        ) = &event
        {
            answer.push_str(&t.text);
        }
    }
    println!("\nrun              : {events} events, final: {answer}");

    // Drain a couple of typed events to prove the sink wired.
    let mut typed = 0usize;
    while typed < 3
        && let Ok(Some(_)) = typed_rx.try_recv()
    {
        typed += 1;
    }
    println!("typed events     : >= {typed} structural events observed");

    println!("\n== done ==");
}
