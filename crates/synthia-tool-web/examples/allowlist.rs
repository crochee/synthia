//! No-network demo: construct `WebFetchTool` with an allowlist,
//! print its parameters schema, and show allowlist decisions.

use std::collections::HashSet;

use synthia_tool::Tool;
use synthia_tool_web::WebFetchTool;

#[tokio::main]
async fn main() {
    let hosts: HashSet<String> =
        ["allowed.example"].iter().map(|s| s.to_string()).collect();
    let tool = WebFetchTool::new().with_allowed_hosts(hosts);

    println!("parameters():");
    println!(
        "{}",
        serde_json::to_string_pretty(&tool.parameters()).unwrap()
    );

    println!(
        "is_url_allowed(https://allowed.example/path) = {}",
        tool.is_url_allowed("https://allowed.example/path")
    );
    println!(
        "is_url_allowed(https://blocked.example/path) = {}",
        tool.is_url_allowed("https://blocked.example/path")
    );
    assert!(tool.is_url_allowed("https://allowed.example/path"));
    assert!(!tool.is_url_allowed("https://blocked.example/path"));
    println!("WEB-OK");
}
