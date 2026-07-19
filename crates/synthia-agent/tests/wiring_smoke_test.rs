#![allow(deprecated)]
use synthia_agent::tools::providers::default_providers;
#[test]
fn default_providers_returns_nine() {
    let providers = default_providers();
    assert_eq!(providers.len(), 9);
    let names: Vec<&str> = providers.iter().map(|p| p.name()).collect();
    assert!(names.contains(&"bash_tools"));
    assert!(names.contains(&"file_tools"));
    assert!(names.contains(&"mcp_tools"));
    assert!(names.contains(&"search_tools"));
    assert!(names.contains(&"guardian"));
    assert!(names.contains(&"monitor"));
    assert!(names.contains(&"external_hook_tool"));
    assert!(names.contains(&"plugin_cli"));
    assert!(names.contains(&"tool_search"));
}
