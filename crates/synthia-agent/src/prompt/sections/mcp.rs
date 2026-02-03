use super::PromptSection;
use crate::prompt::{McpServerInfo, Result, SectionCaching};

#[derive(Debug, Clone)]
pub struct DynamicMcpInstructionsSection {
    servers: Vec<McpServerInfo>,
}

impl DynamicMcpInstructionsSection {
    pub fn new(servers: Vec<McpServerInfo>) -> Self {
        Self { servers }
    }
}

impl Default for DynamicMcpInstructionsSection {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl PromptSection for DynamicMcpInstructionsSection {
    fn name(&self) -> &str {
        "mcp_instructions"
    }

    fn caching(&self) -> SectionCaching {
        SectionCaching::Volatile
    }

    fn build(&self, _ctx: &crate::prompt::PromptContext<'_>) -> Result<String> {
        let servers_with_instructions: Vec<&McpServerInfo> = self
            .servers
            .iter()
            .filter(|s| s.instructions.is_some())
            .collect();

        if servers_with_instructions.is_empty() {
            return Ok(String::new());
        }

        let mut output = String::from(
            "# MCP Server Instructions\n\nThe following MCP servers have provided instructions for how to use their tools and resources:\n\n",
        );

        for server in servers_with_instructions {
            if let Some(ref instructions) = server.instructions {
                output.push_str(&format!(
                    "## {}\n{}\n\n",
                    server.name, instructions
                ));
            }
        }

        Ok(output.trim().to_string())
    }
}
