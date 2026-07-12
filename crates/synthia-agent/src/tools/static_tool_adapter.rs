//! Adapter that wraps a static `synthia_tool::Tool` for use in the dynamic provider system.

use std::sync::Arc;

use async_trait::async_trait;

use crate::tools::dynamic_provider::{
    ToolProvider,
    tool_provider::{SchemaRef, ToolDefinition},
};

/// Wraps a static `synthia_tool::Tool` as a `ToolProvider`.
#[derive(Clone)]
pub struct StaticToolAdapter {
    tool: Arc<dyn synthia_tool::Tool>,
}

impl StaticToolAdapter {
    pub fn new(tool: Arc<dyn synthia_tool::Tool>) -> Self {
        Self { tool }
    }
}

#[async_trait]
impl ToolProvider for StaticToolAdapter {
    fn name(&self) -> &'static str {
        "static_adapter"
    }

    fn list_tools(&self) -> Vec<ToolDefinition> {
        vec![ToolDefinition {
            name: self.tool.name().to_string(),
            description: self.tool.description().to_string(),
            parameters: SchemaRef::Inline(self.tool.parameters()),
            deprecated: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::tools::{
        dynamic_provider::ToolProvider,
        static_tool_adapter::StaticToolAdapter,
    };

    #[tokio::test]
    async fn static_adapter_wraps_read_tool() {
        use synthia_tool::builtin::ReadTool;
        let tool = Arc::new(ReadTool::new());
        let adapter = StaticToolAdapter::new(tool);
        let tools = adapter.list_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "read");
    }

    #[tokio::test]
    async fn static_adapter_provider_name() {
        use synthia_tool::builtin::ReadTool;
        let tool = Arc::new(ReadTool::new());
        let adapter = StaticToolAdapter::new(tool);
        assert_eq!(adapter.name(), "static_adapter");
    }
}
