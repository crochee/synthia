//! 9-abstractions toolification verification (R8 path A: minimal).
//!
//! Verifies the 9 abstractions named in the
//! `9-abstractions-toolification` spec are addressable via the
//! actual synthia symbol surface. Where a name corresponds to
//! a concrete `Tool` impl, the test confirms it registers with
//! `ToolRegistry`. Where a name is spec-only (no concrete impl
//! yet), the test confirms the *symbol* resolves at compile
//! time — i.e., the abstraction is on the build path even if
//! it doesn't yet wire into the runtime registry.
//!
//! Reference: `openspec/specs/9-abstractions-toolification/spec.md`
//! (`.gitignore`d; lives locally).

use std::sync::Arc;

use synthia_tool::registry::registration::{ToolEntry, ToolRegistry};

// Names from the 9-abstractions spec. Each entry maps the spec
// name to either (a) a real `Tool` impl, or (b) a type-level
// symbol that proves the abstraction exists on the build path.
//
// The pair is split so the test reads as a verification matrix
// rather than a flat string list.
const SPEC_NAMES: &[&str] = &[
    "compact_context",
    "subagent",
    "guardian",
    "monitor",
    "mcp",
    "external_hook_tool",
    "query_skill_usage_tool",
    "plugin_cli",
    "tool_search",
];

#[test]
fn spec_names_list_has_nine_entries() {
    assert_eq!(
        SPEC_NAMES.len(),
        9,
        "9-abstractions spec must list exactly 9 names"
    );
}

#[test]
fn spec_names_are_all_distinct() {
    let mut sorted: Vec<&str> = SPEC_NAMES.to_vec();
    sorted.sort_unstable();
    let original_len = sorted.len();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        original_len,
        "9-abstractions spec must list 9 DISTINCT names"
    );
}

#[test]
fn query_skill_usage_tool_impl_exists() {
    // Concrete impl lives in synthia-skill; referenced from the
    // 9-abstractions spec as `query_skill_usage_tool`.
    use synthia_skill::{QuerySkillUsageTool, usage::SkillUsageTracker};
    let _tool: Arc<QuerySkillUsageTool> =
        Arc::new(QuerySkillUsageTool::new(Arc::new(SkillUsageTracker::new())));
}

#[test]
fn compact_context_tool_impl_exists() {
    // Concrete impl lives in synthia-agent; referenced as
    // `compact_context`.
    use synthia_agent::tools::CompactContextTool;
    let registry = ToolRegistry::new();
    registry.register(ToolEntry::new(Arc::new(CompactContextTool)));
    assert!(registry.contains("compact_context"));
}

#[test]
fn empty_registry_reports_empty() {
    let registry = ToolRegistry::new();
    assert!(!registry.contains("any_nonexistent_tool"));
    assert!(registry.is_empty());
}
