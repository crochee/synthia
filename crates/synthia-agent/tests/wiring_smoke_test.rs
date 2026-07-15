use synthia_agent::tools::providers::default_providers;
#[test]
fn default_providers_returns_four() {
    let providers = default_providers();
    assert_eq!(providers.len(), 4);
    let names: Vec<&str> = providers.iter().map(|p| p.name()).collect();
    assert!(names.contains(&"bash_tools"));
    assert!(names.contains(&"file_tools"));
    assert!(names.contains(&"mcp_tools"));
    assert!(names.contains(&"search_tools"));
}
