//! Integration test for `default_providers()` factory.
//!
//! Asserts that the factory assembles the four built-in providers
//! (file, bash, search, MCP) so callers without an explicit provider
//! set get the same baseline.

use synthia_agent::tools::providers::default_providers;

#[test]
fn default_providers_returns_four_providers() {
    let providers = default_providers();

    assert_eq!(
        providers.len(),
        4,
        "expected default_providers() to yield 4 providers, got {}",
        providers.len(),
    );
}

#[test]
fn default_providers_names_match_implemented_providers() {
    let providers = default_providers();

    let mut names: Vec<&'static str> =
        providers.iter().map(|p| p.name()).collect();
    names.sort_unstable();

    let mut expected =
        vec!["bash_tools", "file_tools", "mcp_tools", "search_tools"];
    expected.sort_unstable();

    assert_eq!(
        names, expected,
        "default_providers() returned mismatched provider names",
    );
}
