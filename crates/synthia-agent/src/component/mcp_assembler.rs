use synthia_core::Error;
use synthia_mcp::McpManager;

pub struct McpAssembler {
    server_configs: Vec<synthia_mcp::McpServerConfig>,
}

impl Default for McpAssembler {
    fn default() -> Self {
        Self::new()
    }
}

impl McpAssembler {
    pub fn new() -> Self {
        Self {
            server_configs: Vec::new(),
        }
    }

    pub fn with_configs(
        mut self,
        configs: Vec<synthia_mcp::McpServerConfig>,
    ) -> Self {
        self.server_configs = configs;
        self
    }

    pub async fn assemble(self) -> Result<Option<McpManager>, Error> {
        if self.server_configs.is_empty() {
            return Ok(None);
        }

        let manager = McpManager::new();
        for config in self.server_configs {
            manager.register_config(config).await;
        }
        Ok(Some(manager))
    }
}
