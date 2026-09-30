//! `assemble_with_tier_steering` — R10 lego demonstration.
//!
//! Wires three R10 primitives end-to-end without any network /
//! API key:
//!
//! 1. `ModelTier::Small` — cheap heuristic on a `ModelProviderStub`.
//! 2. `Steering::auto(tier, workspace_root)` — one-line tier-tuned
//!    guard / hint / tracker bundle (traitclaw parity).
//! 3. `AdaptiveRegistry::new(inner_registry, tier)` — tier-capped
//!    LLM-visible tool list (5 tools for `Small`, 32 for `Large`).
//!
//! Also demonstrates the `Router` trait + `LeaderRouter` for
//! `@agent:` text-routed delegation. No peer agent is registered,
//! so the router stays in pass-through mode — the test of the
//! API surface is enough for the demo.
//!
//! Run:
//!
//! ```bash
//! cargo run --example assemble_with_tier_steering -p synthia-harness
//! ```

use std::sync::Arc;

use synthia_provider::{
    ModelProvider,
    ModelTier,
    TierLimits,
    traits_stub::ModelProviderStub,
};
use synthia_steering::Steering;
use synthia_tool::{AdaptiveRegistry, ToolEntry, ToolRegistry};
use synthia_tool_read::ReadTool;
use synthia_tool_shell::ShellTool;
use synthia_tool_task::{LeaderRouter, Router};
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
    println!("== synthia R10: tier-aware steering + adaptive registry ==\n");

    // 1. Provider (stub — no network, no API key). The stub's
    //    ModelConfig has a tiny context_window so the default
    //    `from_model_config` heuristic returns `Small`.
    let provider: Arc<dyn ModelProvider> =
        Arc::new(ModelProviderStub::text_only("stub"));
    let resolved = provider.tier();
    println!(
        "provider tier: {resolved} (effective: {})",
        resolved.clone().effective()
    );
    let limits = TierLimits::for_tier(resolved.clone());
    println!(
        "tier preset:   tool_budget={}, max_visible_tools={}, max_concurrency={}, loop_window={}",
        limits.tool_budget,
        limits.max_visible_tools,
        limits.max_concurrency,
        limits.loop_detection_window,
    );

    // 2. Tool registry + adaptive wrapper.
    let workspace = std::path::PathBuf::from(".");
    let registry: Arc<ToolRegistry> = Arc::new(plugin_tool_registry());
    let adaptive = AdaptiveRegistry::new(registry.clone(), resolved.clone());
    println!(
        "\nAdaptiveRegistry visible tools ({}):",
        adaptive.visible_tool_names().len()
    );
    for name in adaptive.visible_tool_names() {
        println!("  - {name}");
    }

    // 3. Tier-aware steering bundle (one-line config).
    let steering: Arc<Steering> =
        Arc::new(synthia_steering::tier::auto(resolved.clone(), &workspace));
    println!(
        "\nSteering bundle: {} guards, {} hints, concurrency cap = {}",
        steering.guards.len(),
        steering.hints.len(),
        limits.max_concurrency,
    );

    // 4. Router — pass-through for the demo (no peer agents).
    let router: Arc<dyn Router> =
        Arc::new(LeaderRouter::new(Vec::<String>::new()));
    let decision = router.route("@researcher: please investigate X.");
    println!("\nRouter name: {}", router.name());
    println!("Router description: {}", router.description());
    println!(
        "Router decision for '@researcher: please investigate X.': {}",
        if decision.has_mentions() {
            "route"
        } else {
            "pass-through"
        }
    );

    // 5. Override demo: force `Large` to show the cap change.
    let large = ModelTier::Large;
    let large_limits = TierLimits::for_tier(large.clone());
    let _adaptive_large =
        AdaptiveRegistry::new(registry.clone(), large.clone());
    println!(
        "\nForced tier Large: max_visible_tools={}, max_concurrency={}, tool_budget={}",
        large_limits.max_visible_tools,
        large_limits.max_concurrency,
        large_limits.tool_budget,
    );

    println!("\n== done ==");
}
