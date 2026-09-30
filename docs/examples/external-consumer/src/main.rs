//! # Consume synthia from outside the workspace (R18 proof)
//!
//! This crate is deliberately NOT a member of the synthia
//! workspace: it depends on the crates by relative path, exactly
//! as an external library consumer would. If it compiles and
//! runs, the "assemble a complete agent as a lib from zero"
//! claim has direct evidence.
//!
//! ```bash
//! cd docs/examples/external-consumer && cargo run --release
//! ```
//!
//! Expected tail: `CONSUMER-PROOF: OK`
//!
//! ## What it demonstrates (one expression each)
//!
//! | Piece | Crate | Call |
//! |---|---|---|
//! | Provider (stub, no network) | `synthia-provider` | `ModelProviderStub::text_only` |
//! | Tier-aware steering | `synthia-steering` | `tier::auto(provider.tier(), ws)` |
//! | Typed event sink | `synthia-session` | `TypedEventSink::channel(32)` |
//! | Agent assembly | `synthia-harness` | `ReActAgent::new(..)` chain → the agent is its own builder |
//! | Structured output | `synthia-harness`/`synthia-tool` | `.with_output_schema(json!{..})` |
//! | LLM compaction | `synthia-context` | `.with_compaction_settings(default)` |
//! | Multimodal store | `synthia-attachment` | `AttachmentStore::{save,read}_image` |
//! | Runtime-neutral cancel | `synthia-core` | `AtomicCancelToken` (std only) |
//! | Skill registry | `synthia-skill` | `SkillRegistry::new()` |
//!
//! No `tokio` type appears in any of the synthia API calls above —
//! the runtime is the consumer's choice.

use std::sync::Arc;

use futures::StreamExt;
use synthia_harness::agent::Agent as _;
use synthia_harness::{AgentEvent, AgentInput, ReActAgent};
use synthia_context::CompactionSettings;
use synthia_provider::traits::ModelProvider;
use synthia_session::TypedEventSink;
use synthia_steering::tier::auto as tier_steering;
use synthia_tool::{ToolEntry, ToolRegistry};
use synthia_tool_read::ReadTool;
use synthia_tool_shell::ShellTool;
use synthia_tool_todo::TodoWriteTool;
use synthia_tool_write::WriteTool;

/// The tool surface, composed brick by brick: one line per plugin
/// crate, nothing hidden in a default set.
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
    let workspace = std::path::PathBuf::from("/tmp");

    // Piece 1 — provider (stub, no network).
    let provider: Arc<dyn ModelProvider> = Arc::new(
        synthia_provider::traits_stub::ModelProviderStub::text_only(
            "consumer proof ok",
        ),
    );

    // Piece 2 — steering from the provider's tier.
    let steering = Arc::new(tier_steering(provider.tier(), &workspace));

    // Piece 3 — typed event sink (futures mpsc, runtime-neutral).
    let (sink, mut rx) = TypedEventSink::channel(32);

    // Pieces 4..n — one-expression assembly.
    let agent = ReActAgent::new(provider, Arc::new(plugin_tool_registry()))
        .with_name("consumer-proof")
        .with_workspace(&workspace)
        .with_steering(steering)
        .with_output_schema(serde_json::json!({
            "type": "object",
            "properties": {"answer": {"type": "string"}},
            "required": ["answer"]
        }))
        .with_compaction_settings(CompactionSettings::default())
        .with_typed_event_sink(sink);

    println!(
        "descriptor: {} (schema: {})",
        agent.descriptor().name,
        agent.descriptor().output_schema.is_some()
    );

    // Piece n+1 — attachment store (multimodal), used directly.
    let store = synthia_attachment::AttachmentStore::default();
    let png: Vec<u8> = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00,
        0x0D, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
        0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89,
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63,
        0xF8, 0xCF, 0xC0, 0x00, 0x00, 0x00, 0x03, 0x00, 0x01, 0x6F, 0x32,
        0x4D, 0x0E, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
        0x42, 0x60, 0x82,
    ];
    let saved = store.save_image(&png, "image/png").expect("save image");
    let bytes_back = store.read_image(&saved).expect("read image");
    println!(
        "attachment: {} ({} bytes round-tripped)",
        &saved.hash[..12],
        bytes_back.len()
    );

    // Piece n+2 — runtime-neutral cancellation (std-only token).
    let cancel: Arc<dyn synthia_core::CancelToken> =
        Arc::new(synthia_core::AtomicCancelToken::new());

    // Drive a turn with NO tokio type in the public API surface.
    let mut stream = agent
        .run(AgentInput::text("prove the consumer works"), cancel)
        .await;
    let mut events = 0usize;
    let mut text = String::new();
    while let Some(event) = stream.next().await {
        events += 1;
        if let AgentEvent::Model(synthia_provider::ContentPart::Text(t)) = &event
        {
            text.push_str(&t.text);
        }
    }
    println!("run: {events} events, text = {text:?}");

    // Drain typed structural events.
    let mut typed = 0;
    while typed < 3 && matches!(rx.try_recv(), Ok(Some(_))) {
        typed += 1;
    }
    println!("typed events: {typed}");

    // Skill registry (the lego for procedural knowledge).
    let reg = synthia_skill::SkillRegistry::new();
    println!("skill registry providers: {}", reg.provider_names().len());

    assert_eq!(agent.descriptor().name, "consumer-proof");
    assert!(text.contains("consumer proof ok"));
    assert_eq!(bytes_back, png);
    println!("CONSUMER-PROOF: OK");
}
