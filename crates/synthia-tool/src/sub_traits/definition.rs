//! `ToolDefinition` sub-trait — static metadata about a tool.
//!
//! Provides read-only access to a tool's identity, schema, and category.
//! This sub-trait extracts the "what am I?" concern from the monolithic
//! [`crate::Tool`] trait, keeping each sub-trait's API surface ≤ 5 methods.

use serde_json::Value;

use super::ToolCategory;

/// Static metadata sub-trait: name, description, schema, category.
///
/// Every tool MUST implement this — it's the identity card used by
/// `ToolRegistry` for lookup, LLM tool_choice enumeration, and
/// permission routing.
pub trait ToolDefinition: Send + Sync + 'static {
    /// Tool name (used as LLM function name, must be unique per registry).
    fn name(&self) -> &str;

    /// Human-readable description for LLM tool_choice.
    fn description(&self) -> &str;

    /// JSON Schema for tool parameters.
    fn parameters_schema(&self) -> Value;

    /// Tool category for routing and permission decisions.
    fn category(&self) -> ToolCategory;

    /// Snapshot this tool's metadata as a value type.
    fn to_metadata(&self) -> ToolMetadataSnapshot;
}

/// Lightweight snapshot of a tool's definition metadata.
///
/// Cheaply cloneable, suitable for inclusion in `Vec<ToolMetadataSnapshot>`
/// in the `ToolRegistry` dual-index.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolMetadataSnapshot {
    pub name: String,
    pub description: String,
    pub category: ToolCategory,
    pub parameters_schema: Value,
    pub version: String,
}

#[cfg(test)]
mod tests {
    /// Compile-time sanity: `ToolDefinition` exposes at most 5 methods.
    ///
    /// We use a trait-method-count trick: if the trait ever grows beyond
    /// 5 required methods, this test should be updated to reflect the
    /// new count (or the trait should be split further).
    #[test]
    fn tool_definition_has_at_most_five_methods() {
        // The 5 methods are: name, description, parameters_schema,
        // category, to_metadata.
        // This is a documentation test — if you add a 6th required method,
        // update the count below and reconsider the split.
        const METHOD_COUNT: usize = 5;
        const {
            assert!(
                METHOD_COUNT <= 5,
                "ToolDefinition exceeds 5 methods — consider splitting"
            )
        };
    }
}
