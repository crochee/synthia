//! Built-in tool provider for search-style tools.
//!
//! Wraps the `grep` (`GrepTool`) and `glob` (`GlobTool`) tools
//! defined in `synthia-tool`, exposing their static metadata to
//! the dynamic provider framework.

use async_trait::async_trait;

use crate::tools::dynamic_provider::{SchemaRef, ToolDefinition, ToolProvider};

/// Provider for search-style tools: content grep and filesystem glob.
#[derive(Clone)]
pub struct SearchToolsProvider;

impl SearchToolsProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SearchToolsProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolProvider for SearchToolsProvider {
    fn name(&self) -> &'static str {
        "search_tools"
    }

    fn list_tools(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "grep".to_string(),
                description:
                    "Searches for a regular expression pattern in file contents and returns matching lines."
                        .to_string(),
                parameters: SchemaRef::Inline(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Regular expression pattern to search for"
                        },
                        "path": {
                            "type": "string",
                            "description": "File or directory to search in (defaults to workspace root)"
                        },
                        "case_insensitive": {
                            "type": "boolean",
                            "description": "Case insensitive search"
                        },
                        "context_lines": {
                            "type": "integer",
                            "description": "Number of context lines before and after match"
                        },
                        "max_results": {
                            "type": "integer",
                            "description": "Maximum number of results to return"
                        }
                    },
                    "required": ["pattern"]
                })),
                deprecated: None,
            },
            ToolDefinition {
                name: "glob".to_string(),
                description: "Finds files by glob pattern (e.g. '**/*.rs').".to_string(),
                parameters: SchemaRef::Inline(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Glob pattern (e.g., '**/*.rs', 'src/**/*.ts')"
                        },
                        "path": {
                            "type": "string",
                            "description": "Directory to search from (defaults to workspace root)"
                        }
                    },
                    "required": ["pattern"]
                })),
                deprecated: None,
            },
        ]
    }
}
