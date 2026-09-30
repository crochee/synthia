//! Agent-facing `write` tool.
//!
//! This is a self-contained tool plugin depending only on the
//! `synthia-tool` paradigm crate.
//!
//! Create or overwrite a workspace file. Supports two write modes:
//! - `overwrite` (default): replace the file's contents entirely.
//! - `append`: append to the existing file's contents (no-op on
//!   non-existent files; the parent directory is created when
//!   `create_directories = true`).
//!
//! Paths are resolved against `Context::workspace_root` if relative,
//! and must stay inside the workspace — the shared `check_path_safety`
//! guard from `synthia_tool::workspace` is enforced.

use std::path::PathBuf;

use async_trait::async_trait;
use schemars_derive::JsonSchema;
use serde::Deserialize;
use synthia_tool::{
    Context,
    ExecutionMode,
    Tool,
    ToolAnnotations,
    ToolOutput,
    workspace::{check_path_safety, resolve_path},
};

/// Write mode for [`WriteTool`].
///
/// Serializes as `"overwrite"` / `"append"` (snake_case).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
enum WriteMode {
    #[default]
    Overwrite,
    Append,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(extend("additionalProperties" = false))]
struct WriteRequest {
    #[schemars(
        description = "Absolute path, or workspace-relative path, of the file to write."
    )]
    file_path: String,
    #[schemars(description = "Text content to write.")]
    content: String,
    #[serde(default)]
    #[schemars(
        extend("default" = "overwrite"),
        description = "Write mode. `overwrite` (default) replaces the file's contents; `append` appends to the existing file (creates it if missing)."
    )]
    mode: Option<WriteMode>,
    #[serde(default)]
    #[schemars(
        extend("default" = true),
        description = "When true (default), missing parent directories are created automatically. Set to false to require the directory to exist."
    )]
    create_directories: Option<bool>,
}

/// `write` — create or append to a workspace file.
#[derive(Debug, Default)]
pub struct WriteTool;

impl WriteTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for WriteTool {
    fn name(&self) -> &str {
        "write"
    }

    fn description(&self) -> &str {
        "Create or append to a workspace file. Default `mode` is \
         `overwrite` (replaces contents); `mode=append` appends to the \
         existing file. With `create_directories=true` (default), missing \
         parent directories are created automatically."
    }

    fn parameters(&self) -> serde_json::Value {
        // Schema is generated from `WriteRequest` via `schemars`,
        // so the type and the LLM-facing schema cannot drift —
        // including `additionalProperties: false` and the
        // `mode` / `create_directories` defaults, all declared
        // inline via `#[schemars(extend(...))]`.
        serde_json::to_value(schemars::schema_for!(WriteRequest))
            .expect("WriteRequest schema is always serializable")
    }

    /// R17: write results are path + byte-count; a UI can show
    /// a compact "wrote N bytes to P" card.
    fn output_definition(&self) -> synthia_tool::output::ToolOutputDefinition {
        synthia_tool::output::ToolOutputDefinition::passthrough("write")
            .with_kind(synthia_tool::output::RenderKind::Write)
            .with_title("Write")
    }

    /// Mutating tool: the loop must never run two of these at once.
    ///
    /// `mode = "append"` is a read-modify-write (`read_to_string` →
    /// build → `fs::write`), so two calls in one LLM pass sharing a
    /// target path would lose one write — reproduced against the real
    /// tool: two concurrent `append` calls to one path left only the
    /// second body on disk. `mode = "overwrite"` is one truncate+write
    /// per call, so a reader can observe the intermediate empty file.
    /// [`ExecutionMode::Sequential`] is the framework's answer to both:
    /// the loop runs the Parallel bucket to completion, then this bucket
    /// strictly one call at a time — the same rule `shell`, `web_fetch`
    /// and the MCP proxy tools follow.
    fn mode(&self) -> ExecutionMode {
        ExecutionMode::Sequential
    }

    /// R124: MCP-native descriptor hints for permission gates.
    /// `write` mutates the workspace; `append` is not idempotent
    /// (each call concatenates), `overwrite` is destructive (each
    /// call replaces), so the conservative aggregate is
    /// `destructive = true, idempotent = false`. No network.
    fn annotations(&self) -> Option<ToolAnnotations> {
        Some(ToolAnnotations {
            read_only_hint: Some(false),
            destructive_hint: Some(true),
            idempotent_hint: Some(false),
            open_world_hint: Some(false),
        })
    }

    async fn call(
        &self,
        input: serde_json::Value,
        context: &Context,
    ) -> ToolOutput {
        let request: WriteRequest = match serde_json::from_value(input) {
            Ok(r) => r,
            Err(e) => {
                return ToolOutput::error(format!("Invalid arguments: {e}"));
            }
        };

        if let Some(err) =
            check_path_safety(&context.workspace_root, &request.file_path)
        {
            return ToolOutput::error(err);
        }

        let mode = request.mode.unwrap_or_default();
        let should_create_dirs = request.create_directories.unwrap_or(true);
        let resolved: PathBuf =
            resolve_path(&context.workspace_root, &request.file_path);

        if should_create_dirs
            && let Some(parent) = resolved.parent()
            && !parent.as_os_str().is_empty()
            && !parent.exists()
            && let Err(e) = tokio::fs::create_dir_all(parent).await
        {
            return ToolOutput::error(format!(
                "Failed to create parent directory '{}': {}",
                parent.display(),
                e
            ));
        }

        let existed = resolved.exists();
        let new_text = if mode == WriteMode::Append {
            let prior = if existed {
                match tokio::fs::read_to_string(&resolved).await {
                    Ok(s) => s,
                    Err(e) => {
                        return ToolOutput::error(format!(
                            "Failed to read existing file '{}' for append: {}",
                            resolved.display(),
                            e
                        ));
                    }
                }
            } else {
                String::new()
            };
            format!("{prior}{}", request.content)
        } else {
            request.content
        };

        if let Err(e) = tokio::fs::write(&resolved, new_text.as_bytes()).await {
            return ToolOutput::error(format!(
                "Failed to write file '{}': {}",
                resolved.display(),
                e
            ));
        }

        let action = if existed && mode == WriteMode::Append {
            "Appended to"
        } else if existed {
            "Updated"
        } else {
            "Created"
        };
        ToolOutput::text(format!("{action} file: {}", resolved.display()))
    }
}

#[cfg(test)]
mod tests;
