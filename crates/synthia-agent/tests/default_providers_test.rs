//! Integration test for `default_providers()` factory.
//!
//! Asserts that the factory assembles the nine built-in providers
//! (file, bash, search, MCP, plus the five R11 spec-only shells:
//! guardian, monitor, external_hook_tool, plugin_cli, tool_search)
//! so callers without an explicit provider set get the same baseline.

use synthia_agent::tools::providers::default_providers;

#[test]
fn default_providers_returns_four_providers() {
    let providers = default_providers();

    assert_eq!(
        providers.len(),
        9,
        "expected default_providers() to yield 9 providers, got {}",
        providers.len(),
    );
}

#[test]
fn default_providers_names_match_implemented_providers() {
    let providers = default_providers();

    let mut names: Vec<&'static str> =
        providers.iter().map(|p| p.name()).collect();
    names.sort_unstable();

    let mut expected = vec![
        "bash_tools",
        "external_hook_tool",
        "file_tools",
        "guardian",
        "mcp_tools",
        "monitor",
        "plugin_cli",
        "search_tools",
        "tool_search",
    ];
    expected.sort_unstable();

    assert_eq!(
        names, expected,
        "default_providers() returned mismatched provider names",
    );
}
