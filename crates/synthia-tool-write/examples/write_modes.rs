//! Demonstrates `WriteTool` overwrite and append modes against a
//! tempdir workspace, then reads the file back with `std::fs`.

use synthia_tool::{Context, Tool};
use synthia_tool_write::WriteTool;

#[tokio::main]
async fn main() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let context = Context::new("demo".to_string(), root.clone());
    let tool = WriteTool::new();

    let out = tool
        .call(
            serde_json::json!({
                "file_path": "notes.txt",
                "content": "first line\n"
            }),
            &context,
        )
        .await;
    println!("overwrite -> is_error={:?}", out.is_error);
    for c in &out.content {
        if let Some(t) = c.text() {
            println!("overwrite -> {t}");
        }
    }

    let out = tool
        .call(
            serde_json::json!({
                "file_path": "notes.txt",
                "content": "second line\n",
                "mode": "append"
            }),
            &context,
        )
        .await;
    println!("append    -> is_error={:?}", out.is_error);
    for c in &out.content {
        if let Some(t) = c.text() {
            println!("append    -> {t}");
        }
    }

    let body = std::fs::read_to_string(root.join("notes.txt")).unwrap();
    println!("file body -> {body:?}");
    assert_eq!(body, "first line\nsecond line\n");
    println!("WRITE-OK");
}
