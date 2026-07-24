use synthia_provider::ToolDefinition;

use super::{Source, SourceContent, SourceDelta, SourceId};

/// Tracks tool schema content via canonical JSON serialization.
///
/// Tool definitions are sorted by name before serialization so that the
/// baseline is independent of input order.
pub struct ToolSchemasSource {
    id: SourceId,
    baseline_content: SourceContent,
    current_tools: Vec<ToolDefinition>,
}

impl ToolSchemasSource {
    fn canonical(tools: &[ToolDefinition]) -> SourceContent {
        let mut sorted: Vec<&ToolDefinition> = tools.iter().collect();
        sorted.sort_by(|a, b| a.name.cmp(&b.name));
        let json = serde_json::to_string(&sorted).unwrap_or_default();
        SourceContent::from_text(&json)
    }

    /// Create a new source with the given tools as the baseline.
    pub fn new(tools: &[ToolDefinition]) -> Self {
        let baseline_content = Self::canonical(tools);
        Self {
            id: SourceId("tool-schemas"),
            baseline_content,
            current_tools: tools.to_vec(),
        }
    }

    /// Update the current tool set. Call [`update`](Source::update) afterwards
    /// to get the delta.
    pub fn set_tools(&mut self, tools: &[ToolDefinition]) {
        self.current_tools = tools.to_vec();
    }
}

impl Source for ToolSchemasSource {
    fn id(&self) -> SourceId {
        self.id.clone()
    }

    fn baseline(&self) -> SourceContent {
        self.baseline_content.clone()
    }

    fn update(&mut self) -> SourceDelta {
        let new_content = Self::canonical(&self.current_tools);
        if new_content.hash() == self.baseline_content.hash() {
            SourceDelta::Unchanged
        } else {
            SourceDelta::Changed(new_content)
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn tool(name: &str) -> ToolDefinition {
        ToolDefinition::new(name, "desc", json!({}))
    }

    #[test]
    fn id_is_tool_schemas() {
        let source = ToolSchemasSource::new(&[tool("a")]);
        assert_eq!(source.id(), SourceId("tool-schemas"));
    }

    #[test]
    fn update_returns_unchanged_for_same_tools() {
        let mut source = ToolSchemasSource::new(&[tool("a"), tool("b")]);
        assert!(matches!(source.update(), SourceDelta::Unchanged));
    }

    #[test]
    fn update_returns_changed_when_tools_differ() {
        let mut source = ToolSchemasSource::new(&[tool("a")]);
        source.set_tools(&[tool("a"), tool("b")]);
        assert!(matches!(source.update(), SourceDelta::Changed(_)));
    }

    #[test]
    fn canonical_order_is_independent_of_input_order() {
        let a = ToolSchemasSource::new(&[tool("a"), tool("b")]);
        let b = ToolSchemasSource::new(&[tool("b"), tool("a")]);
        assert_eq!(a.baseline().hash(), b.baseline().hash());
    }
}
