//! Agent-facing `write` tool.
//!
//! Create or overwrite a workspace file. Supports two write modes:
//! - `overwrite` (default): replace the file's contents entirely.
//! - `append`: append to the existing file's contents (no-op on
//!   non-existent files; the parent directory is created when
//!   `create_directories = true`).
//!
//! Paths are resolved against `Context::workspace_root` if relative,
//! and must stay inside the workspace — the same `check_path_safety`
//! guard used by [`crate::builtin::read`] is enforced.

use std::path::PathBuf;

use async_trait::async_trait;
use schemars_derive::JsonSchema;
use serde::Deserialize;

use crate::{
    builtin::read::{check_path_safety, resolve_path},
    traits::Tool,
    types::{Context, ToolOutput},
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

/// Mirror of [`crate::builtin::read::resolve_path`] so this module
/// compiles when `read` is the only caller of the original. (Avoids
/// duplicating the implementation; the helper is already `pub(super)`
/// within `crate::builtin`.)
#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn make_context(root: PathBuf) -> Context {
        Context::new("s1".to_string(), root)
    }

    fn first_text(out: &ToolOutput) -> String {
        out.content
            .iter()
            .find_map(|c| c.text().map(str::to_string))
            .unwrap_or_default()
    }

    // ---- Tool metadata --------------------------------------------

    #[test]
    fn tool_uses_agent_facing_name() {
        let tool = WriteTool::new();
        assert_eq!(tool.name(), "write");
    }

    #[test]
    fn parameters_require_path_and_content() {
        let tool = WriteTool::new();
        let params = tool.parameters();
        let required = params["required"]
            .as_array()
            .expect("required should be a list");
        let labels: Vec<&str> =
            required.iter().filter_map(|v| v.as_str()).collect();
        assert!(labels.contains(&"file_path"));
        assert!(labels.contains(&"content"));
    }

    /// Pin the JSON-Schema shape for `write` so future drift in
    /// either the schema, the typed `WriteRequest`, or the runtime
    /// defaults (which the schema mirrors) is caught by a failing
    /// test rather than silent runtime confusion.
    #[test]
    fn parameters_schema_is_self_consistent() {
        let tool = WriteTool::new();
        let params = tool.parameters();
        assert_eq!(params["type"], "object");

        let mut required: Vec<&str> = params["required"]
            .as_array()
            .expect("required")
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        required.sort_unstable();
        assert_eq!(required, vec!["content", "file_path"]);

        let props = params["properties"].as_object().expect("properties");

        let file_path = &props["file_path"];
        assert_eq!(file_path["type"], "string");
        assert!(
            file_path["description"].as_str().is_some(),
            "file_path must carry a description"
        );

        let content = &props["content"];
        assert_eq!(content["type"], "string");
        assert!(
            content["description"].as_str().is_some(),
            "content must carry a description"
        );

        let mode = &props["mode"];
        // `Option<WriteMode>` → schemars emits `anyOf` with the
        // enum ref plus `{"type": "null"}`. Validate the enum ref
        // by walking the `anyOf` branches.
        let any_of = mode["anyOf"]
            .as_array()
            .expect("mode should use anyOf for Option<enum>");
        let mut found_enum_ref = false;
        let mut found_null = false;
        for branch in any_of {
            if branch
                .get("$ref")
                .and_then(|v| v.as_str())
                .is_some_and(|s| s.ends_with("/WriteMode"))
            {
                found_enum_ref = true;
            }
            if branch.get("type").and_then(|v| v.as_str()) == Some("null") {
                found_null = true;
            }
        }
        assert!(found_enum_ref, "mode anyOf should $ref the WriteMode enum");
        assert!(found_null, "mode anyOf should include a null branch");

        // schemars 1.x follows JSON Schema draft 2020-12 which
        // uses `$defs` (not the legacy `definitions` key) for
        // referenced subschemas. Pin the WriteMode enum values here.
        let defs = params["$defs"].as_object().expect("$defs");
        let write_mode_def =
            defs["WriteMode"].as_object().expect("WriteMode def");
        assert_eq!(write_mode_def["type"], "string");
        let mode_enum: Vec<&str> = write_mode_def["enum"]
            .as_array()
            .expect("mode enum")
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert_eq!(mode_enum, vec!["overwrite", "append"]);

        // Runtime default must be visible in the schema.
        assert_eq!(
            mode["default"], "overwrite",
            "mode schema default must match runtime default"
        );

        let create_dirs = &props["create_directories"];
        // `Option<bool>` → schemars emits `type: ["boolean", "null"]`.
        let cd_ty = &create_dirs["type"];
        let cd_ty_ok = cd_ty == "boolean"
            || cd_ty.as_array().is_some_and(|arr| {
                arr.iter().any(|v| v == "boolean")
                    && arr.iter().any(|v| v == "null")
            });
        assert!(
            cd_ty_ok,
            "create_directories type should be boolean or [boolean, null], got: {cd_ty}"
        );
        assert_eq!(
            create_dirs["default"], true,
            "create_directories schema default must match runtime default"
        );

        // `mode` and `create_directories` are optional.
        assert!(!required.contains(&"mode"));
        assert!(!required.contains(&"create_directories"));

        assert_eq!(
            params["additionalProperties"], false,
            "additional fields must be rejected to match serde_json::from_value"
        );
    }

    /// Schema defaults for `mode` and `create_directories` MUST
    /// match the typed `WriteRequest` runtime defaults — drift here
    /// would mean the LLM is told one behavior but the tool does
    /// another.
    #[tokio::test]
    async fn schema_defaults_match_runtime_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let tool = WriteTool::new();

        // Omit `mode` → must behave like overwrite.
        let p = dir.path().join("default-mode.txt");
        let out = tool
            .call(
                json!({"file_path": p.to_str().unwrap(), "content": "x"}),
                &make_context(dir.path().to_path_buf()),
            )
            .await;
        let _ = out; // success body asserted by creates_new_file

        // Omit `create_directories` → parent dir should be created.
        let p = dir.path().join("nested/deep/file.txt");
        let out = tool
            .call(
                json!({"file_path": p.to_str().unwrap(), "content": "y"}),
                &make_context(dir.path().to_path_buf()),
            )
            .await;
        assert!(out.is_error.is_none());
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "y");
    }

    // ---- Write behavior -------------------------------------------

    #[tokio::test]
    async fn creates_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let tool = WriteTool::new();
        let p = dir.path().join("a.txt");
        let out = tool
            .call(
                json!({"file_path": p.to_str().unwrap(), "content": "hello"}),
                &make_context(dir.path().to_path_buf()),
            )
            .await;
        assert!(
            out.is_error.is_none(),
            "expected ok, got: {first_text:?}",
            first_text = first_text(&out)
        );
        let body = std::fs::read_to_string(&p).unwrap();
        assert_eq!(body, "hello");
        let msg = first_text(&out);
        assert!(msg.contains("Created"), "got: {msg}");
    }

    #[tokio::test]
    async fn overwrite_replaces_existing_contents() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.txt");
        std::fs::write(&p, "before").unwrap();
        let tool = WriteTool::new();
        let out = tool
            .call(
                json!({"file_path": p.to_str().unwrap(), "content": "after"}),
                &make_context(dir.path().to_path_buf()),
            )
            .await;
        assert!(out.is_error.is_none());
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "after");
        let msg = first_text(&out);
        assert!(msg.contains("Updated"), "got: {msg}");
    }

    #[tokio::test]
    async fn append_concatenates_to_existing_contents() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.txt");
        std::fs::write(&p, "before\n").unwrap();
        let tool = WriteTool::new();
        let out = tool
            .call(
                json!({
                    "file_path": p.to_str().unwrap(),
                    "content": "after\n",
                    "mode": "append"
                }),
                &make_context(dir.path().to_path_buf()),
            )
            .await;
        assert!(out.is_error.is_none());
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "before\nafter\n");
        let msg = first_text(&out);
        assert!(msg.contains("Appended"), "got: {msg}");
    }

    #[tokio::test]
    async fn append_to_missing_file_creates_it() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("fresh.txt");
        let tool = WriteTool::new();
        let out = tool
            .call(
                json!({
                    "file_path": p.to_str().unwrap(),
                    "content": "hi",
                    "mode": "append"
                }),
                &make_context(dir.path().to_path_buf()),
            )
            .await;
        assert!(out.is_error.is_none());
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "hi");
        let msg = first_text(&out);
        assert!(msg.contains("Created"), "got: {msg}");
    }

    #[tokio::test]
    async fn creates_missing_parent_directories_by_default() {
        let dir = tempfile::tempdir().unwrap();
        let tool = WriteTool::new();
        let p = dir.path().join("a/b/c.txt");
        let out = tool
            .call(
                json!({"file_path": p.to_str().unwrap(), "content": "deep"}),
                &make_context(dir.path().to_path_buf()),
            )
            .await;
        assert!(out.is_error.is_none());
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "deep");
    }

    #[tokio::test]
    async fn create_directories_false_surfaces_write_error() {
        let dir = tempfile::tempdir().unwrap();
        let tool = WriteTool::new();
        let p = dir.path().join("a/b/c.txt");
        let out = tool
            .call(
                json!({
                    "file_path": p.to_str().unwrap(),
                    "content": "deep",
                    "create_directories": false
                }),
                &make_context(dir.path().to_path_buf()),
            )
            .await;
        assert!(out.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn relative_path_resolves_against_workspace_root() {
        let dir = tempfile::tempdir().unwrap();
        let tool = WriteTool::new();
        let out = tool
            .call(
                json!({"file_path": "rel.txt", "content": "x"}),
                &make_context(dir.path().to_path_buf()),
            )
            .await;
        assert!(out.is_error.is_none());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("rel.txt")).unwrap(),
            "x"
        );
    }

    #[tokio::test]
    async fn path_traversal_blocked() {
        let dir = tempfile::tempdir().unwrap();
        let tool = WriteTool::new();
        let out = tool
            .call(
                json!({
                    "file_path": "../../../tmp/escape.txt",
                    "content": "x"
                }),
                &make_context(dir.path().to_path_buf()),
            )
            .await;
        assert!(out.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn invalid_mode_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let tool = WriteTool::new();
        let p = dir.path().join("a.txt");
        let out = tool
            .call(
                json!({
                    "file_path": p.to_str().unwrap(),
                    "content": "x",
                    "mode": "truncate"
                }),
                &make_context(dir.path().to_path_buf()),
            )
            .await;
        assert!(out.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn missing_arguments_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let tool = WriteTool::new();
        let out = tool
            .call(
                json!({"file_path": "x.txt"}),
                &make_context(dir.path().to_path_buf()),
            )
            .await;
        assert!(out.is_error.unwrap_or(false));
    }
}
