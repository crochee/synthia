//! Demonstrates `ReadTool` line-range reads against a temp workspace.

use synthia_tool::Tool;
use synthia_tool_read::ReadTool;

#[tokio::main]
async fn main() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ten.txt");
    let body: String = (1..=10).map(|i| format!("line {i}\n")).collect();
    std::fs::write(&file, body).unwrap();

    let context = synthia_tool::Context::new(
        "example".to_string(),
        dir.path().to_path_buf(),
    );
    let tool = ReadTool::new();
    let input = serde_json::json!({
        "file_path": "ten.txt",
        "offset": 3,
        "limit": 4,
    });
    let out = tool.call(input, &context).await;
    let text = out
        .content
        .iter()
        .find_map(|c| c.text().map(str::to_string))
        .unwrap_or_default();
    print!("{text}");
    assert!(text.contains("   3 line 3"));
    assert!(text.contains("   6 line 6"));
    assert!(!text.contains("line 2\n"));
    assert!(!text.contains("line 7"));
    println!("READ-OK");
}
