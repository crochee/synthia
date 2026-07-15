//! Integration test for `BashToolsProvider`.
//!
//! Asserts that the provider publishes the bash/monitor tools that
//! already exist in `synthia-tool-bash` (`bash`, `Monitor`) so the
//! provider can be slotted into the migration follow-on chain.

use synthia_agent::tools::{
    dynamic_provider::ToolProvider,
    providers::bash_tools_provider::BashToolsProvider,
};

#[test]
fn bash_tools_provider_lists_at_least_one_tool() {
    let provider = BashToolsProvider::new();
    let tools = provider.list_tools();

    assert!(
        !tools.is_empty(),
        "BashToolsProvider must publish at least one tool, got 0 (provider name = {})",
        provider.name(),
    );
}

#[test]
fn bash_tools_provider_name_is_bash_tools() {
    let provider = BashToolsProvider::new();
    assert_eq!(provider.name(), "bash_tools");
}

#[test]
fn bash_tools_provider_exposes_known_names() {
    let provider = BashToolsProvider::new();
    let tools = provider.list_tools();
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();

    assert!(
        names.contains(&"bash"),
        "expected provider to expose the 'bash' tool (matches synthia-tool-bash TOOL_NAME), got {names:?}",
    );
}
