//! # Assemble an agent using the typed `ProviderProfile` (R9-1)
//!
//! Run it:
//!
//! ```text
//! cargo run --example assemble_with_provider_profile -p synthia-harness
//! ```
//!
//! Demonstrates how the new `synthia_provider::ProviderProfile`
//! typed enum replaces the string-keyed `WorkspaceConfig` for
//! in-memory assembly. Three pieces are wired into one running
//! agent:
//!
//! | # | Piece | Crate | Public API used |
//! |---|-------|-------|-----------------|
//! | 1 | Provider profile | `synthia-provider` | `ProviderProfile::OpenAI` / `Anthropic` / `Stub`, `ProviderRegistry` |
//! | 2 | Agent | `synthia-harness` | `ReActAgent::new(..).with_steering(..)` |
//! | 3 | HookMap | `synthia-steering` | state-aware `on_with_state` |
//!
//! ## Runtime neutrality
//!
//! Provider profile is a pure config primitive — no runtime
//! `Arc<dyn CancelToken>` and `futures::Stream` contracts as
//! every other example.

use std::sync::Arc;

use synthia_core::AtomicCancelToken;
use synthia_harness::{Agent, AgentEvent, AgentInput, ReActAgent};
use synthia_provider::{
    AnthropicProfile,
    ModelProvider,
    OpenAIProfile,
    ProviderProfile,
    ProviderRegistry,
    StubProfile,
};
use synthia_session::TypedEventSink;
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
    println!("=== synthia: assemble with typed ProviderProfile ===\n");

    // -----------------------------------------------------------------
    // Piece 1 — typed provider profile. Three profiles are
    // registered and the default is selected **explicitly**: the
    // stub, so this example runs with no credential in the
    // environment. Without `with_default` the registry would pick
    // the first profile inserted, and building an Anthropic or
    // OpenAI profile without a key is a hard error by design (that
    // error path is demonstrated below).
    // -----------------------------------------------------------------
    let registry = ProviderRegistry::new()
        .insert(ProviderProfile::Anthropic(
            AnthropicProfile::claude_sonnet_4(),
        ))
        .insert(ProviderProfile::OpenAI(OpenAIProfile::openai_mini()))
        .insert(ProviderProfile::Stub(StubProfile::text_only()))
        .with_default("stub");

    // Print the registered profile kinds + the chosen default.
    println!("[1/3] profile registry");
    for name in registry.names() {
        let kind = registry.get(&name).map(|p| p.kind()).unwrap_or("?");
        println!("      - {name} ({kind})");
    }
    println!(
        "      default: {} (kind={})",
        registry
            .default_profile()
            .map(|p| p.name())
            .unwrap_or("<none>"),
        registry.default_profile().map(|p| p.kind()).unwrap_or("?")
    );

    // Build the chosen default profile into a real
    // `Box<dyn ModelProvider>`. Stubs always build; the
    // OpenAI / Anthropic variants will fail-fast if no API key
    // is set (we demonstrate that error path with the
    // openai-mini entry below).
    let provider: Box<dyn ModelProvider> = registry
        .build_default()
        .expect("the stub profile builds without an API key");
    println!("      built default provider: {}", provider.name());
    let provider: Arc<dyn ModelProvider> = Arc::from(provider);

    // The error path: openai-mini requires an OPENAI_API_KEY.
    match registry.build("openai-mini") {
        Ok(_) => println!("      openai-mini: built (OPENAI_API_KEY was set)"),
        Err(err) => println!("      openai-mini: skipped ({})", err,),
    }

    // -----------------------------------------------------------------
    // Piece 2 — state-aware hook map. The hook vetoes a `shell`
    // tool call when the iteration index crosses 3 (dynamic
    // guard, pi `harness/hooks.ts` parity).
    // -----------------------------------------------------------------
    let hook_map =
        HookMap::new().on_with_state("before_tool", |event, state| {
            if let HookEvent::BeforeTool { tool_name, .. } = event
                && tool_name == "shell"
                && state.iteration_count >= 3
            {
                return HookDecision::Block {
                    reason: "shell blocked after 3 iterations".to_string(),
                };
            }
            HookDecision::Allow
        });
    let mut state = synthia_context::AgentState::with_window(100);
    let probe_low = hook_map.dispatch_with_state(
        &HookEvent::BeforeTool {
            tool_name: "shell".to_string(),
            call_id: "c1".to_string(),
            arguments: serde_json::json!({}),
        },
        &state,
    );
    println!("      iter=0   : {probe_low:?}");
    state.iteration_count = 3;
    let probe_high = hook_map.dispatch_with_state(
        &HookEvent::BeforeTool {
            tool_name: "shell".to_string(),
            call_id: "c1".to_string(),
            arguments: serde_json::json!({}),
        },
        &state,
    );
    println!("      iter=3   : {probe_high:?}");
    println!("[2/3] state-aware HookMap");

    // -----------------------------------------------------------------
    // Piece 3 — agent. Same ReActAgent as every other example,
    // but the provider came from a `ProviderRegistry` rather
    // than from `WorkspaceConfig`.
    // -----------------------------------------------------------------
    let workspace = std::path::PathBuf::from(".");
    let tool_registry: Arc<ToolRegistry> = Arc::new(plugin_tool_registry());
    let steering = Arc::new(Steering::default_policy(&workspace));

    let (typed_sink, _typed_rx) = TypedEventSink::channel(64);
    let agent = ReActAgent::new(provider, tool_registry)
        .with_steering(steering)
        .with_typed_event_sink(typed_sink);

    let cancel = AtomicCancelToken::shared();
    let mut stream = agent.run(AgentInput::text("hello"), cancel.clone()).await;
    use futures::StreamExt;
    let mut answer = String::new();
    while let Some(event) = stream.next().await {
        if let AgentEvent::Model(content) = &event
            && let synthia_provider::ContentPart::Text(text) = content
        {
            answer.push_str(&text.text);
        }
    }
    println!("[3/3] agent run");
    println!("      answer: {answer}");
    println!("\ndone.");
}
