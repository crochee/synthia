//! # Assemble a complete agent from synthia's lego pieces
//!
//! Run it:
//!
//! ```text
//! cargo run --example assemble_from_scratch -p synthia-harness
//! ```
//!
//! This example is the executable form of the composition
//! tutorial. It wires **seven independent pieces** into a running
//! agent and shows the three surfaces a lib consumer interacts
//! with:
//!
//! | # | Piece | Crate | Neutral? |
//! |---|-------|-------|----------|
//! | 1 | `ModelProvider` | `synthia-provider` | yes — implement your own, emit `Stream<Result<StreamChunk, Error>>` |
//! | 2 | `ToolRegistry` | `synthia-tool` | yes |
//! | 3 | `Steering` (guards / hooks / hints / tracker) | `synthia-steering` | yes |
//! | 4 | `ContextManager` | `synthia-context` | yes (`prepare_arc` keeps the cache Arc) |
//! | 5 | `TypedEventSink` | `synthia-session` | yes — `futures` channel, not tokio |
//! | 6 | `CancelToken` | `synthia-core` | yes — `AtomicCancelToken` is std-only |
//! | 7 | `ReActAgent` | `synthia-harness` | yes — `Agent::run` takes `Arc<dyn CancelToken>` |
//!
//! ## Runtime neutrality
//!
//! The library public API names no runtime: cancellation is the
//! `synthia_core::CancelToken` trait, streams are `futures::Stream`,
//! the typed-event channel is `futures::channel::mpsc`. This
//! example still needs **an** executor to poll the futures, so it
//! picks tokio for convenience — while deliberately passing an
//! [`AtomicCancelToken`] (a std-only token, *not*
//! `tokio_util::CancellationToken`) to prove the token type is
//! decoupled from the executor. A tokio consumer can pass its
//! `CancellationToken` directly: the bridge impl coerces it to
//! `Arc<dyn CancelToken>` with no adapter.

use std::{path::PathBuf, sync::Arc};

use futures::StreamExt;
use synthia_core::{AtomicCancelToken, CancelToken, Registry};
use synthia_harness::{Agent, AgentEvent, AgentInput, ReActAgent};
use synthia_provider::{
    BlockAssembler,
    ContentPart,
    TextContent,
    traits_stub::ModelProviderStub,
};
use synthia_session::{KNOWN_SESSION_EVENT_TYPES, TypedEventSink};
use synthia_steering::{HookDecision, HookEvent, HookMap, Steering};
use synthia_tool::{ToolEntry, ToolRegistry};
use synthia_tool_read::ReadTool;
use synthia_tool_shell::ShellTool;
use synthia_tool_todo::TodoWriteTool;
use synthia_tool_write::WriteTool;

/// The tool surface, composed brick by brick from the plugin
/// crates — no hidden default set.
fn plugin_tool_registry() -> ToolRegistry {
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(ReadTool::new())));
    registry.register_entry(ToolEntry::new(Arc::new(WriteTool::new())));
    registry.register_entry(ToolEntry::new(Arc::new(ShellTool::new())));
    registry.register_entry(ToolEntry::new(Arc::new(TodoWriteTool::new())));
    registry
}

#[tokio::main]
async fn main() {
    println!("=== synthia: assemble an agent from scratch ===\n");

    // -----------------------------------------------------------------
    // Piece 1 — the model. Any `ModelProvider` works; the stub yields
    // a single scripted answer so this example needs no network.
    // -----------------------------------------------------------------
    let provider: Arc<dyn synthia_provider::traits::ModelProvider> =
        Arc::new(ModelProviderStub::text_only(
            "Hello from a fully assembled synthia agent.",
        ));
    println!("[1/7] provider      : {}", provider.name());

    // -----------------------------------------------------------------
    // Piece 2 — tools. No default set ships: `plugin_tool_registry()`
    // below registers one brick per plugin crate (read / write /
    // shell / todo), all rooted at the workspace via `Context`.
    // -----------------------------------------------------------------
    let workspace = PathBuf::from(".");
    let registry: Arc<ToolRegistry> = Arc::new(plugin_tool_registry());
    let tool_names = registry
        .list(None)
        .await
        .map(|entries| {
            entries
                .iter()
                .map(|e| {
                    use synthia_core::registry::RegistryItem;
                    e.name().to_string()
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    println!("[2/7] tools         : {tool_names:?}");

    // -----------------------------------------------------------------
    // Piece 3 — steering. `default_policy` installs the defensive
    // guards / hints / tracker; a `HookMap` adds name-keyed typed
    // hooks on top.
    // -----------------------------------------------------------------
    let hook_map = HookMap::new().on("before_tool", |event| match event {
        HookEvent::BeforeTool { tool_name, .. } if tool_name == "shell" => {
            HookDecision::Block {
                reason: "example: shell blocked by HookMap".to_string(),
            }
        }
        _ => HookDecision::Allow,
    });
    let steering = Arc::new(Steering::default_policy(&workspace));
    println!(
        "[3/7] steering      : {} guards, {} hooks, HookMap(1 handler)",
        steering.guards.len(),
        steering.hooks.len(),
    );
    // Demonstrate the registry side of HookMap in isolation.
    let probe = hook_map.dispatch(&HookEvent::BeforeTool {
        tool_name: "shell".to_string(),
        call_id: "c1".to_string(),
        arguments: serde_json::json!({}),
    });
    println!("      hook decision : {probe:?}");

    // -----------------------------------------------------------------
    // Piece 4 — the context manager budgets the message list against
    // the model window before every LLM call. `TruncatingContextManager`
    // is the default; swap in `SummarizingContextManager` (LLM
    // summaries) or `DagContextManager` without touching the loop.
    // -----------------------------------------------------------------
    let context_manager: std::sync::Arc<dyn synthia_context::ContextManager> =
        std::sync::Arc::new(synthia_context::TruncatingContextManager);
    println!("[4/7] context mgr   : TruncatingContextManager");

    // -----------------------------------------------------------------
    // Piece 5 — the typed-event channel. Structural boundary events
    // (request_header / iteration / step) travel this futures mpsc;
    // the receiver drains them into whatever durable log you own.
    // -----------------------------------------------------------------
    let (typed_sink, mut typed_rx) = TypedEventSink::channel(64);
    println!("[5/7] typed channel : capacity 64 (futures mpsc)");

    // -----------------------------------------------------------------
    // Piece 7 — assemble the agent. Each `with_*` installs one
    // previously-built piece; nothing is hard-wired.
    // -----------------------------------------------------------------
    let agent = ReActAgent::new(provider, registry)
        .with_steering(steering)
        .with_context_manager(context_manager)
        .with_max_iterations(6)
        .with_typed_event_sink(typed_sink.clone());
    println!("[7/7] agent         : ReActAgent(max_iterations=6)\n");

    // -----------------------------------------------------------------
    // Piece 6 — cancellation. A std-only token: no runtime types in
    // the agent's public contract.
    // -----------------------------------------------------------------
    let cancel: Arc<dyn CancelToken> = AtomicCancelToken::shared();
    println!("[6/7] cancel token  : AtomicCancelToken (std-only)");
    println!("      cancelled?    : {}\n", cancel.is_cancelled());

    // -----------------------------------------------------------------
    // Drive the agent and consume both surfaces.
    // -----------------------------------------------------------------
    println!("--- run ---");
    let mut stream = agent
        .run(AgentInput::text("Say hello, briefly."), cancel)
        .await;
    let mut model_chars = 0usize;
    let mut model_text = String::new();
    let mut system_kinds: Vec<&'static str> = Vec::new();
    while let Some(event) = stream.next().await {
        match &event {
            AgentEvent::Model(ContentPart::Text(TextContent {
                text, ..
            })) => {
                model_chars += text.chars().count();
                model_text.push_str(text);
            }
            AgentEvent::ModelDone(result) => {
                println!("      model      : {} chars", model_chars);
                println!("      answer     : {}", model_text.trim());
                println!("      tool calls : {}", result.tool_calls.len());
            }
            AgentEvent::System(sys) => {
                system_kinds.push(sys.kind());
            }
            _ => {}
        }
    }
    println!("      lifecycle  : {system_kinds:?}");

    // Structural events the loop published. `try_recv` is the
    // synchronous drain — every event is buffered before the run's
    // event stream ends, so no task or await is required.
    println!("\n--- typed structural events ---");
    while let Ok(Some(record)) = typed_rx.try_recv() {
        let session_event = record.event;
        println!(
            "      {:<15} {}",
            session_event.type_tag(),
            compact(session_event)
        );
    }
    println!(
        "\nknown typed event vocabulary ({}): {KNOWN_SESSION_EVENT_TYPES:?}",
        KNOWN_SESSION_EVENT_TYPES.len()
    );

    // -----------------------------------------------------------------
    // Piece 4 — the context manager also runs standalone. The
    // `BlockAssembler` is likewise an independent lego brick: feed
    // it any provider chunk stream and read the folded message.
    // -----------------------------------------------------------------
    println!("\n--- standalone pieces ---");
    let mut assembler = BlockAssembler::new();
    assembler.push(synthia_provider::StreamChunk::Content(ContentPart::Text(
        TextContent {
            text: "folded ".to_string(),
            cache_control: None,
        },
    )));
    assembler.push(synthia_provider::StreamChunk::Content(ContentPart::Text(
        TextContent {
            text: "text".to_string(),
            cache_control: None,
        },
    )));
    let (sampling, parts, incomplete) = assembler.finalize_assistant();
    println!(
        "      BlockAssembler : text={:?} parts={} incomplete={incomplete}",
        sampling.text,
        parts.len()
    );

    println!("\n=== assembled agent ran end-to-end ===");
}

/// One-line JSON rendering of a typed event, for console output.
fn compact(event: synthia_session::SessionEvent) -> String {
    serde_json::to_string(&event).unwrap_or_default()
}
