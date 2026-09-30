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

/// R124: pin the MCP-native descriptor hints — destructive
/// (mutates the workspace), not idempotent (append concatenates),
/// no outbound network.
#[test]
fn annotations_are_mcp_native_destructive() {
    let tool = WriteTool::new();
    let a = tool
        .annotations()
        .expect("write declares MCP-style annotations");
    assert_eq!(a.read_only_hint, Some(false));
    assert_eq!(a.destructive_hint, Some(true));
    assert_eq!(a.idempotent_hint, Some(false));
    assert_eq!(a.open_world_hint, Some(false));
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

/// The tool MUST advertise `Sequential`.
///
/// `mode = "append"` reads the file, builds the new text, and writes
/// it back — a read-modify-write. Two `write` calls in one LLM pass
/// that share a target path lose one of them when run concurrently
/// (reproduced against the real tool: two `join!`-ed appends left only
/// the second body). Optimising this tool into the Parallel bucket is
/// therefore a **behaviour change**, not an internal detail: the loop's
/// `join_all` would interleave them. The serialization itself is proven
/// in `synthia-harness`'s
/// `parallel_dispatch::unsafe_tools_run_serially_even_when_safe_present`.
#[test]
fn write_is_sequential_so_two_calls_cannot_race_a_path() {
    assert_eq!(WriteTool::new().mode(), ExecutionMode::Sequential);
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
    let write_mode_def = defs["WriteMode"].as_object().expect("WriteMode def");
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
