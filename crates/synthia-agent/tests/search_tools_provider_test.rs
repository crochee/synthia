#![allow(deprecated)]
//! Integration test for `SearchToolsProvider`.
//!
//! Asserts that the provider publishes the grep/glob tools that
//! already exist in `synthia-tool` (`grep`, `glob`) so the provider
//! can be slotted into the migration follow-on chain.

use synthia_agent::tools::{
    dynamic_provider::ToolProvider,
    providers::search_tools_provider::SearchToolsProvider,
};

#[test]
fn search_tools_provider_lists_at_least_one_tool() {
    let provider = SearchToolsProvider::new();
    let tools = provider.list_tools();

    assert!(
        !tools.is_empty(),
        "SearchToolsProvider must publish at least one tool, got 0 (provider name = {})",
        provider.name(),
    );
}

#[test]
fn search_tools_provider_name_is_search_tools() {
    let provider = SearchToolsProvider::new();
    assert_eq!(provider.name(), "search_tools");
}

#[test]
fn search_tools_provider_exposes_known_names() {
    let provider = SearchToolsProvider::new();
    let tools = provider.list_tools();
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();

    assert!(
        names.contains(&"grep"),
        "expected provider to expose the 'grep' tool (matches synthia-tool GrepTool TOOL_NAME), got {names:?}",
    );
    assert!(
        names.contains(&"glob"),
        "expected provider to expose the 'glob' tool (matches synthia-tool GlobTool TOOL_NAME), got {names:?}",
    );
}
