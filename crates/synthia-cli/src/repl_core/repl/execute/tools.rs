use synthia_core::{Registry, RegistryItem};
use synthia_provider::ToolDefinition;

pub(super) async fn handle_tools() {
    let tool_registry = synthia_tool::registry::ToolRegistry::new();
    let tools: Vec<_> = tool_registry
        .list(None)
        .await
        .map(|entries| {
            entries
                .iter()
                .map(|e| ToolDefinition {
                    name: e.name().to_string(),
                    description: e.description().to_string(),
                    input_schema: e.tool_instance().parameters(),
                    cache_control: None,
                })
                .collect()
        })
        .unwrap_or_default();
    if tools.is_empty() {
        println!("No tools registered.");
    } else {
        println!("Available tools:");
        for tool in &tools {
            println!(
                "  - {}: {}",
                tool.name,
                tool.description.lines().next().unwrap_or("")
            );
        }
    }
}
