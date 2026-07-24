//! Unit tests for the `apply_patch` module family.
//!
//! Coverage map (12 tests):
//!
//! - Per-op happy path: 3 tests
//!   (add, update, delete).
//! - Multi-op happy path: 1 test
//!   (3 ops in one patch: Update + Add + Delete).
//! - Partial failure: 1 test
//!   (codex scenario 015 — Add succeeds, Update of missing file
//!   fails, created.txt must remain).
//! - Move rejection: 1 test
//!   (`*** Move to:` rejected when `enable_move = false`).
//! - Path safety: 1 test
//!   (`../escape.txt` blocked by `check_path_safety`).
//! - Edge cases: 3 tests
//!   (empty patch rejected, `*** Add File:` overwrites existing
//!   file per codex scenario 011, `*** Delete File:` on a
//!   directory is blocked).
//! - Registration + metadata: 2 tests
//!   (default registry contains `apply_patch`, tool metadata
//!   (name, requires_permission, !is_concurrency_safe)).

use std::{path::PathBuf, sync::Arc};

use tokio_util::sync::CancellationToken;

use super::*;
use crate::{
    FileChangeEvent,
    traits::Tool,
    types::{ToolExecutionContext, ToolInput},
};

fn make_input(workspace_root: PathBuf, input: serde_json::Value) -> ToolInput {
    ToolInput {
        name: "apply_patch".to_string(),
        input,
        context: ToolExecutionContext::new("s1".to_string(), workspace_root),
    }
}

#[tokio::test]
async fn test_add_file() {
    let dir = tempfile::tempdir().unwrap();
    let tool = ApplyPatchTool::default();
    let input = make_input(
        dir.path().to_path_buf(),
        serde_json::json!({
            "patch": "*** Begin Patch\n*** Add File: new.txt\n+hello\n+world\n*** End Patch\n"
        }),
    );
    let output = tool.call(input).await;
    assert!(
        !output.is_error.unwrap_or(false),
        "got error: {:?}",
        output.content
    );
    let created = dir.path().join("new.txt");
    assert!(created.exists());
    assert_eq!(std::fs::read_to_string(&created).unwrap(), "hello\nworld\n");
}

#[tokio::test]
async fn test_update_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.txt");
    std::fs::write(&file, "alpha\nbeta\ngamma\n").unwrap();
    let tool = ApplyPatchTool::default();
    let input = make_input(
        dir.path().to_path_buf(),
        serde_json::json!({
            "patch": "*** Begin Patch\n*** Update File: test.txt\n@@\n-alpha\n+ALPHA\n*** End Patch\n"
        }),
    );
    let output = tool.call(input).await;
    assert!(
        !output.is_error.unwrap_or(false),
        "got error: {:?}",
        output.content
    );
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "ALPHA\nbeta\ngamma\n"
    );
}

#[tokio::test]
async fn test_delete_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("trash.txt");
    std::fs::write(&file, "bye").unwrap();
    let tool = ApplyPatchTool::default();
    let input = make_input(
        dir.path().to_path_buf(),
        serde_json::json!({
            "patch": "*** Begin Patch\n*** Delete File: trash.txt\n*** End Patch\n"
        }),
    );
    let output = tool.call(input).await;
    assert!(
        !output.is_error.unwrap_or(false),
        "got error: {:?}",
        output.content
    );
    assert!(!file.exists());
}

#[tokio::test]
async fn test_multiple_operations_all_succeed() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    let c = dir.path().join("c.txt");
    std::fs::write(&a, "1\n").unwrap(); // include trailing newline
    std::fs::write(&b, "2").unwrap();
    std::fs::write(&c, "3").unwrap();

    let tool = ApplyPatchTool::default();
    let patch = "*** Begin Patch\n\
                 *** Update File: a.txt\n@@\n-1\n+one\n\
                 *** Add File: new.txt\n+new\n\
                 *** Delete File: c.txt\n\
                 *** End Patch\n";
    let input = make_input(
        dir.path().to_path_buf(),
        serde_json::json!({ "patch": patch }),
    );
    let output = tool.call(input).await;
    assert!(
        !output.is_error.unwrap_or(false),
        "got error: {:?}",
        output.content
    );
    assert_eq!(std::fs::read_to_string(&a).unwrap(), "one\n");
    assert_eq!(std::fs::read_to_string(&b).unwrap(), "2"); // untouched (no trailing newline)
    assert!(!c.exists());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("new.txt")).unwrap(),
        "new\n"
    );
}

#[tokio::test]
async fn test_partial_failure_leaves_applied() {
    // Codex scenario 015: Add succeeds, Update of missing file fails
    let dir = tempfile::tempdir().unwrap();
    let tool = ApplyPatchTool::default();
    let patch = "*** Begin Patch\n\
                 *** Add File: created.txt\n+hello\n\
                 *** Update File: missing.txt\n@@\n-old\n+new\n\
                 *** End Patch\n";
    let input = make_input(
        dir.path().to_path_buf(),
        serde_json::json!({ "patch": patch }),
    );
    let output = tool.call(input).await;
    assert!(output.is_error.unwrap_or(false));
    // created.txt should still exist (partial success)
    let created = dir.path().join("created.txt");
    assert!(
        created.exists(),
        "created.txt must remain after partial failure"
    );
    assert_eq!(std::fs::read_to_string(&created).unwrap(), "hello\n");
}

#[tokio::test]
async fn test_move_rejected_by_default() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("old.txt");
    std::fs::write(&file, "x").unwrap();
    let tool = ApplyPatchTool::default();
    let patch = "*** Begin Patch\n\
                 *** Update File: old.txt\n\
                 *** Move to: new.txt\n\
                 @@\n-x\n+X\n\
                 *** End Patch\n";
    let input = make_input(
        dir.path().to_path_buf(),
        serde_json::json!({ "patch": patch }),
    );
    let output = tool.call(input).await;
    assert!(output.is_error.unwrap_or(false));
    let msg = output.content[0].text().unwrap();
    assert!(msg.contains("moves are not supported"));
}

#[tokio::test]
async fn test_path_traversal_blocked() {
    let dir = tempfile::tempdir().unwrap();
    let tool = ApplyPatchTool::default();
    let patch = "*** Begin Patch\n\
                 *** Add File: ../escape.txt\n+evil\n\
                 *** End Patch\n";
    let input = make_input(
        dir.path().to_path_buf(),
        serde_json::json!({ "patch": patch }),
    );
    let output = tool.call(input).await;
    assert!(output.is_error.unwrap_or(false));
    let msg = output.content[0].text().unwrap();
    assert!(msg.contains("outside workspace"));
}

#[tokio::test]
async fn test_rejects_empty_patch() {
    let dir = tempfile::tempdir().unwrap();
    let tool = ApplyPatchTool::default();
    let input = make_input(
        dir.path().to_path_buf(),
        serde_json::json!({ "patch": "*** Begin Patch\n*** End Patch\n" }),
    );
    let output = tool.call(input).await;
    assert!(output.is_error.unwrap_or(false));
}

#[tokio::test]
async fn test_registers_in_default_registry() {
    let registry =
        crate::registry::registration::ToolRegistry::register_defaults();
    assert!(registry.contains("apply_patch"));
}

#[tokio::test]
async fn test_requires_permission() {
    let tool = ApplyPatchTool::default();
    assert!(tool.requires_permission());
    assert!(!tool.is_concurrency_safe());
    assert_eq!(tool.name(), "apply_patch");
}

#[tokio::test]
async fn test_add_overwrites_existing() {
    // Codex scenario 011: `*** Add File:` is allowed to overwrite an
    // existing file (no special distinction from create in V4A grammar).
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("x.txt");
    std::fs::write(&file, "old").unwrap();
    let tool = ApplyPatchTool::default();
    let patch = "*** Begin Patch\n\
                 *** Add File: x.txt\n+new\n\
                 *** End Patch\n";
    let input = make_input(
        dir.path().to_path_buf(),
        serde_json::json!({ "patch": patch }),
    );
    let output = tool.call(input).await;
    assert!(
        !output.is_error.unwrap_or(false),
        "got error: {:?}",
        output.content
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "new\n");
}

#[tokio::test]
async fn test_delete_directory_blocked() {
    let dir = tempfile::tempdir().unwrap();
    let sub = dir.path().join("subdir");
    std::fs::create_dir(&sub).unwrap();
    let tool = ApplyPatchTool::default();
    let patch = format!(
        "*** Begin Patch\n*** Delete File: {}\n*** End Patch\n",
        sub.file_name().unwrap().to_str().unwrap()
    );
    let input = make_input(
        dir.path().to_path_buf(),
        serde_json::json!({ "patch": patch }),
    );
    let output = tool.call(input).await;
    assert!(output.is_error.unwrap_or(false));
    assert!(sub.exists());
}

#[tokio::test]
async fn test_update_emits_file_change_events_per_hunk() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("events.txt");
    std::fs::write(&file, "alpha\nbeta\ngamma\n").unwrap();

    let events = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let events_clone = events.clone();
    let callback: Arc<dyn Fn(FileChangeEvent) + Send + Sync> =
        Arc::new(move |event| events_clone.lock().push(event));

    let tool = ApplyPatchTool::default();
    let input = make_input(
        dir.path().to_path_buf(),
        serde_json::json!({
            "patch": "*** Begin Patch\n\
                      *** Update File: events.txt\n\
                      @@\n-alpha\n+ALPHA\n\
                      @@\n-gamma\n+GAMMA\n\
                      *** End Patch\n"
        }),
    );
    let output = tool
        .call_with_progress(input, callback, &CancellationToken::new())
        .await;
    assert!(
        !output.is_error.unwrap_or(false),
        "got error: {:?}",
        output.content
    );

    let collected: Vec<FileChangeEvent> = events.lock().clone();
    assert_eq!(collected.len(), 3);
    assert!(matches!(
        &collected[0],
        FileChangeEvent::HunkApplied { path, hunk_index: 0 } if path.ends_with("events.txt")
    ));
    assert!(matches!(
        &collected[1],
        FileChangeEvent::HunkApplied { path, hunk_index: 1 } if path.ends_with("events.txt")
    ));
    assert!(matches!(
        &collected[2],
        FileChangeEvent::FileUpdated { path } if path.ends_with("events.txt")
    ));
}
