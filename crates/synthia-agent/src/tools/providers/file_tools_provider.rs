//! Built-in tool providers for migrating static tools.

use async_trait::async_trait;

use crate::tools::dynamic_provider::{SchemaRef, ToolDefinition, ToolProvider};

/// Provider for file system tools: read_file, write_file, search_files, apply_patch.
#[derive(Clone)]
pub struct FileToolsProvider;

impl FileToolsProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FileToolsProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolProvider for FileToolsProvider {
    fn name(&self) -> &'static str {
        "file_tools"
    }

    fn list_tools(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "read_file".to_string(),
                description: "Read contents of a file".to_string(),
                parameters: SchemaRef::Inline(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "The absolute path to the file to read."
                        },
                        "offset": {
                            "type": "integer",
                            "minimum": 1,
                            "description": "The line number to start reading from (must be at least 1)."
                        },
                        "limit": {
                            "type": "integer",
                            "minimum": 1,
                            "description": "The number of lines to read."
                        }
                    },
                    "required": ["file_path"]
                })),
                deprecated: None,
            },
            ToolDefinition {
                name: "write_file".to_string(),
                description: "Write content to a file".to_string(),
                parameters: SchemaRef::Inline(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "File path to write to"
                        },
                        "content": {
                            "type": "string",
                            "description": "Content to write"
                        },
                        "append": {
                            "type": "boolean",
                            "description": "If true, append to file instead of overwriting"
                        }
                    },
                    "required": ["path", "content"]
                })),
                deprecated: None,
            },
            ToolDefinition {
                name: "search_files".to_string(),
                description: "Search for files matching a pattern".to_string(),
                parameters: SchemaRef::Inline(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "glob": {
                            "type": "string",
                            "description": "Glob pattern to find files (e.g., '**/*.rs')"
                        },
                        "pattern": {
                            "type": "string",
                            "description": "Regular expression pattern to search for in file contents"
                        },
                        "path": {
                            "type": "string",
                            "description": "Directory or file to search in (defaults to workspace root)"
                        },
                        "case_insensitive": {
                            "type": "boolean",
                            "description": "Case insensitive content search"
                        },
                        "context_lines": {
                            "type": "integer",
                            "description": "Number of context lines before and after a content match"
                        },
                        "max_results": {
                            "type": "integer",
                            "description": "Maximum number of content search results to return"
                        }
                    }
                })),
                deprecated: None,
            },
            ToolDefinition {
                name: "apply_patch".to_string(),
                description: "Apply a patch to a file".to_string(),
                parameters: SchemaRef::Inline(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "patch": {
                            "type": "string",
                            "description": "V4A patch text starting with '*** Begin Patch'"
                        }
                    },
                    "required": ["patch"]
                })),
                deprecated: None,
            },
        ]
    }
}
