use synthia_core::Error;
use synthia_tool::ToolRegistry;

pub struct ToolAssembler {
    registry: Option<ToolRegistry>,
}

impl Default for ToolAssembler {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolAssembler {
    pub fn new() -> Self {
        Self { registry: None }
    }

    pub fn with_registry(mut self, registry: ToolRegistry) -> Self {
        self.registry = Some(registry);
        self
    }

    pub async fn assemble(self) -> Result<ToolRegistry, Error> {
        Ok(self.registry.unwrap_or_default())
    }
}
