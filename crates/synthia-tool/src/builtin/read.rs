//! Agent-facing `read` tool.
//!
//! Reads a UTF-8 text file within the workspace, optionally restricted
//! to a 1-based line range. The output is prefixed with right-aligned
//! line numbers so the LLM can reference specific lines back. A UTF-8
//! BOM at the start of the file is stripped before rendering.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use schemars_derive::JsonSchema;
use serde::Deserialize;

use crate::{
    traits::Tool,
    types::{Context, ToolOutput},
};

const UTF8_BOM: &[u8] = b"\xef\xbb\xbf";

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(extend("additionalProperties" = false))]
struct ReadRequest {
    #[schemars(
        description = "Absolute path, or workspace-relative path, of the file to read."
    )]
    file_path: String,
    #[serde(default)]
    #[schemars(
        range(min = 1),
        description = "1-based line number to start reading from. Only provide when the file is too large to read at once."
    )]
    offset: Option<u64>,
    #[serde(default)]
    #[schemars(
        range(min = 1),
        description = "Number of lines to read. Only provide when the file is too large to read at once."
    )]
    limit: Option<u64>,
}

/// `read` — read a workspace file with optional line range.
#[derive(Debug, Default)]
pub struct ReadTool {
    /// Paths the tool has already read this process. Diagnostic
    /// only — useful for future "edit-after-read" enforcement.
    read_history: parking_lot::Mutex<Vec<String>>,
}

impl ReadTool {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that a path was read. Exposed for tests and for callers
    /// that want to track which files the agent has inspected.
    pub fn mark_read(&self, path: &str) {
        self.read_history.lock().push(path.to_string());
    }
}

pub(super) fn resolve_path(workspace_root: &Path, path: &str) -> PathBuf {
    let p = PathBuf::from(path);
    if p.is_absolute() {
        p
    } else {
        workspace_root.join(p)
    }
}

/// Canonicalize a path even if it (or some ancestor) does not exist on disk.
/// This walks the path component-by-component, canonicalizing each existing
/// prefix. For the non-existing tail, components are appended literally.
///
/// Returns the canonical path if it can be determined, otherwise the
/// lexical-resolved path (which preserves `..` components for the
/// `starts_with` check below to catch).
fn safe_canonicalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        result.push(component.as_os_str());
        // Try to canonicalize what we have so far; if it exists, replace
        // `result` with its canonical form (which resolves any `..`).
        if let Ok(canon) = result.canonicalize() {
            result = canon;
        }
    }
    result
}

pub(super) fn check_path_safety(
    workspace_root: &Path,
    path: &str,
) -> Option<String> {
    let resolved = resolve_path(workspace_root, path);
    let canonical_root = workspace_root
        .canonicalize()
        .unwrap_or_else(|_| workspace_root.to_path_buf());
    let canonical_path = safe_canonicalize(&resolved);
    if !canonical_path.starts_with(&canonical_root) {
        return Some(format!("Path {path} is outside workspace"));
    }
    None
}

#[async_trait]
impl Tool for ReadTool {
    fn name(&self) -> &str {
        "read"
    }

    fn description(&self) -> &str {
        "Reads a UTF-8 text file from the workspace. Returns lines \
         prefixed with their 1-based line number; supports an optional \
         `offset` (1-based start line) and `limit` (number of lines). \
         A leading UTF-8 BOM is stripped automatically."
    }

    fn parameters(&self) -> serde_json::Value {
        // Schema is generated from `ReadRequest` via `schemars`,
        // so the type and the LLM-facing schema cannot drift —
        // including `additionalProperties: false`, which is
        // declared inline via `#[schemars(extend(...))]` on the
        // struct.
        serde_json::to_value(schemars::schema_for!(ReadRequest))
            .expect("ReadRequest schema is always serializable")
    }

    async fn call(
        &self,
        input: serde_json::Value,
        context: &Context,
    ) -> ToolOutput {
        let request: ReadRequest = match serde_json::from_value(input) {
            Ok(r) => r,
            Err(e) => {
                return ToolOutput::error(format!("Invalid arguments: {e}"));
            }
        };

        let workspace_root = &context.workspace_root;
        if let Some(err) = check_path_safety(workspace_root, &request.file_path)
        {
            return ToolOutput::error(err);
        }
        let resolved = resolve_path(workspace_root, &request.file_path);

        let offset = request.offset.map(|v| v as usize);
        let limit = request.limit.map(|v| v as usize);

        let bytes = match tokio::fs::read(&resolved).await {
            Ok(b) => b,
            Err(e) => {
                return ToolOutput::error(format!(
                    "Failed to read file '{}': {}",
                    resolved.display(),
                    e
                ));
            }
        };

        let bytes = if bytes.starts_with(UTF8_BOM) {
            &bytes[UTF8_BOM.len()..]
        } else {
            &bytes[..]
        };

        let content = match String::from_utf8(bytes.to_vec()) {
            Ok(c) => c,
            Err(e) => {
                return ToolOutput::error(format!(
                    "File '{}' is not valid UTF-8: {}",
                    resolved.display(),
                    e
                ));
            }
        };

        self.mark_read(&request.file_path);

        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        let start = offset.unwrap_or(1).saturating_sub(1);
        let end = match (offset, limit) {
            (Some(_), Some(l)) => start + l,
            (Some(_), None) => lines.len(),
            (None, Some(l)) => l,
            (None, None) => lines.len(),
        };

        let selected_lines =
            &lines[start.min(total_lines)..end.min(total_lines)];
        let mut output = String::new();
        for (i, line) in selected_lines.iter().enumerate() {
            let line_num = start + i + 1;
            output.push_str(&format!("{line_num:>4} {line}\n"));
        }

        if output.is_empty() && selected_lines.is_empty() {
            ToolOutput::text(format!(
                "(file '{}' is empty)",
                resolved.display()
            ))
        } else {
            ToolOutput::text(output)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::json;

    use super::*;
    use crate::types::Context;

    fn make_context(root: PathBuf) -> Context {
        Context::new("s1".to_string(), root)
    }

    fn make_input(file_path: &str) -> serde_json::Value {
        serde_json::json!({"file_path": file_path})
    }

    fn make_input_with_args(
        file_path: &str,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        map.insert("file_path".to_string(), serde_json::json!(file_path));
        if let Some(o) = offset {
            map.insert("offset".to_string(), serde_json::json!(o));
        }
        if let Some(l) = limit {
            map.insert("limit".to_string(), serde_json::json!(l));
        }
        serde_json::Value::Object(map)
    }

    fn first_text(out: &ToolOutput) -> String {
        out.content
            .iter()
            .find_map(|c| c.text().map(str::to_string))
            .unwrap_or_default()
    }

    // ---- Tool metadata --------------------------------------------

    #[tokio::test]
    async fn tool_uses_agent_facing_name() {
        let tool = ReadTool::new();
        assert_eq!(tool.name(), "read");
    }

    /// Pin the JSON-Schema shape for `read` so future drift in
    /// either the schema or the typed `ReadRequest` is caught by a
    /// failing test rather than silent runtime confusion.
    #[test]
    fn parameters_schema_is_self_consistent() {
        let tool = ReadTool::new();
        let params = tool.parameters();
        assert_eq!(params["type"], "object");

        let mut required: Vec<&str> = params["required"]
            .as_array()
            .expect("required")
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        required.sort_unstable();
        assert_eq!(required, vec!["file_path"]);

        let props = params["properties"].as_object().expect("properties");

        let file_path = &props["file_path"];
        assert_eq!(file_path["type"], "string");
        assert!(
            file_path["description"].as_str().is_some(),
            "file_path must carry a description"
        );

        for key in ["offset", "limit"] {
            let p = &props[key];
            // `Option<u64>` → schemars generates `{"type": ["integer",
            // "null"]}` (nullable pattern). Accept either the
            // single-type or nullable-array form so the test isn't
            // tied to schemars' exact emission.
            let ty = &p["type"];
            let ty_ok = ty == "integer"
                || ty.as_array().is_some_and(|arr| {
                    arr.iter().any(|v| v == "integer")
                        && arr.iter().any(|v| v == "null")
                });
            assert!(
                ty_ok,
                "{key} type should be integer or [integer, null], got: {ty}"
            );
            assert_eq!(p["minimum"].as_f64(), Some(1.0));
            assert!(
                p["description"].as_str().is_some(),
                "{key} must carry a description"
            );
            assert!(!required.contains(&key), "{key} must not be in required");
        }

        assert_eq!(
            params["additionalProperties"], false,
            "additional fields must be rejected to match serde_json::from_value"
        );
    }

    #[tokio::test]
    async fn missing_file_path_argument_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let tool = ReadTool::new();
        let out = tool
            .call(
                json!({"offset": 1}),
                &make_context(dir.path().to_path_buf()),
            )
            .await;
        assert!(out.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn wrong_offset_type_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let tool = ReadTool::new();
        let out = tool
            .call(
                json!({"file_path": "x.txt", "offset": "not-a-number"}),
                &make_context(dir.path().to_path_buf()),
            )
            .await;
        assert!(out.is_error.unwrap_or(false));
    }

    // ---- Read behavior --------------------------------------------

    #[tokio::test]
    async fn reads_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("hello.txt");
        std::fs::write(&p, "line 1\nline 2\nline 3\n").unwrap();
        let tool = ReadTool::new();
        let out = tool
            .call(
                make_input(p.to_str().unwrap()),
                &make_context(dir.path().to_path_buf()),
            )
            .await;
        let text = first_text(&out);
        assert!(text.contains("line 1"));
        assert!(text.contains("line 3"));
    }

    #[tokio::test]
    async fn missing_file_returns_error() {
        let tool = ReadTool::new();
        let out = tool
            .call(
                make_input("/nonexistent/path.txt"),
                &make_context(PathBuf::from("/tmp")),
            )
            .await;
        assert!(out.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn line_range_filters_output() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("lines.txt");
        std::fs::write(&p, "a\nb\nc\nd\ne\n").unwrap();
        let tool = ReadTool::new();
        let out = tool
            .call(
                make_input_with_args("lines.txt", Some(2), Some(3)),
                &make_context(dir.path().to_path_buf()),
            )
            .await;
        let text = first_text(&out);
        assert!(text.contains("   2 b"));
        assert!(text.contains("   3 c"));
        assert!(text.contains("   4 d"));
        assert!(!text.contains("   1 a"));
        assert!(!text.contains("   5 e"));
    }

    #[tokio::test]
    async fn utf8_bom_stripped() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("bom.txt");
        let mut bytes = Vec::from(UTF8_BOM);
        bytes.extend_from_slice(b"hello world");
        std::fs::write(&p, &bytes).unwrap();
        let tool = ReadTool::new();
        let out = tool
            .call(
                make_input_with_args("bom.txt", None, None),
                &make_context(dir.path().to_path_buf()),
            )
            .await;
        let text = first_text(&out);
        assert!(text.contains("hello world"));
        assert!(!text.contains('\u{feff}'));
    }

    #[tokio::test]
    async fn path_traversal_blocked() {
        let dir = tempfile::tempdir().unwrap();
        let tool = ReadTool::new();
        let out = tool
            .call(
                make_input_with_args("../../../etc/passwd", None, None),
                &make_context(dir.path().to_path_buf()),
            )
            .await;
        assert!(out.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn empty_file_renders_placeholder() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("empty.txt");
        std::fs::write(&p, "").unwrap();
        let tool = ReadTool::new();
        let out = tool
            .call(
                make_input(p.to_str().unwrap()),
                &make_context(dir.path().to_path_buf()),
            )
            .await;
        let text = first_text(&out);
        assert!(
            text.contains("is empty"),
            "expected empty-file placeholder, got: {text}"
        );
    }

    // ---- Path resolution helpers ----------------------------------

    #[test]
    fn resolve_relative_joins_workspace_root() {
        let root = Path::new("/workspace");
        let resolved = resolve_path(root, "src/lib.rs");
        assert_eq!(resolved, PathBuf::from("/workspace/src/lib.rs"));
    }

    #[test]
    fn resolve_absolute_passes_through() {
        let root = Path::new("/workspace");
        let resolved = resolve_path(root, "/other/file.txt");
        assert_eq!(resolved, PathBuf::from("/other/file.txt"));
    }

    #[test]
    fn check_path_safety_allows_files_inside_workspace() {
        let dir = tempfile::tempdir().unwrap();
        assert!(check_path_safety(dir.path(), "file.txt").is_none());
    }

    #[test]
    fn check_path_safety_blocks_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let result = check_path_safety(dir.path(), "../../../etc/passwd");
        assert!(result.is_some());
        assert!(result.unwrap().contains("outside workspace"));
    }
}
