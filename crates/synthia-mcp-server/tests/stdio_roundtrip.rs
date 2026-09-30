//! End-to-end test: spawn the `synthia-mcp-server` binary, drive
//! the full MCP wire (`initialize` / `tools/list` / `tools/call`)
//! over its stdin / stdout, and assert every plugin-brick tool is
//! reachable.
//!
//! This is the "give it to pi" proof: a foreign MCP client (a
//! Claude Code plugin, `pi-mcp-adapter`, …) wires this binary as a
//! subprocess and drives exactly the JSON-RPC envelopes tested
//! here. The line protocol the test asserts on is the line protocol
//! those clients speak.

use std::{path::PathBuf, process::Stdio};

use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
};

/// Resolve the binary path for the integration test.
///
/// `CARGO_BIN_EXE_synthia-mcp-server` is set by Cargo's build
/// script for any test that lives in the same crate as the
/// binary. Falling back to `target/debug/synthia-mcp-server` keeps
/// `cargo test --no-run` + manual invocations working.
fn binary_path() -> PathBuf {
    if let Some(path) = option_env!("CARGO_BIN_EXE_synthia-mcp-server") {
        return PathBuf::from(path);
    }
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("..");
    path.push("..");
    path.push("target");
    path.push("debug");
    path.push("synthia-mcp-server");
    path
}

struct Process {
    child: tokio::process::Child,
    stdin: tokio::process::ChildStdin,
    stdout: BufReader<tokio::process::ChildStdout>,
    next_id: u64,
}

impl Process {
    async fn spawn(workspace: &TempDir) -> Self {
        let mut child = Command::new(binary_path())
            .arg("--workspace")
            .arg(workspace.path())
            .arg("--name")
            .arg("test")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn synthia-mcp-server");
        let stdin = child.stdin.take().expect("stdin pipe");
        let stdout = child.stdout.take().expect("stdout pipe");
        // Drain stderr in a background task so it never blocks the
        // child on a full pipe.
        let stderr = child.stderr.take().expect("stderr pipe");
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut buf = String::new();
            loop {
                buf.clear();
                if reader.read_line(&mut buf).await.unwrap_or(0) == 0 {
                    return;
                }
                eprintln!("[server-stderr] {buf}");
            }
        });
        Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 1,
        }
    }

    async fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let envelope = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let line = serde_json::to_string(&envelope).unwrap();
        self.stdin.write_all(line.as_bytes()).await.unwrap();
        self.stdin.write_all(b"\n").await.unwrap();
        self.stdin.flush().await.unwrap();
        loop {
            let mut response_line = String::new();
            let n = self.stdout.read_line(&mut response_line).await.unwrap();
            assert!(n > 0, "server closed stdout before responding");
            let trimmed = response_line.trim();
            let value: Value = serde_json::from_str(trimmed)
                .unwrap_or_else(|e| panic!("parse {trimmed:?}: {e}"));
            // Notifications (no `id`) are dropped — they cannot
            // arrive on the request channel, but the loop is here
            // to be defensive against a future protocol addition.
            if value.get("id").is_some() {
                return value;
            }
        }
    }

    async fn shutdown(mut self) {
        // Closing stdin tells the server the client hung up; the
        // request loop returns Ok(()) and the child exits cleanly.
        drop(self.stdin);
        let _ = self.child.wait().await;
    }
}

#[tokio::test]
async fn initialize_advertises_protocol_version_and_tools() {
    let workspace = tempfile::tempdir().unwrap();
    let mut proc = Process::spawn(&workspace).await;

    let init = proc
        .request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "clientInfo": {
                    "name": "integration-test",
                    "version": "0.0.1",
                },
                "capabilities": {},
            }),
        )
        .await;
    let result = init.get("result").expect("initialize returns a result");
    assert_eq!(
        result["protocolVersion"], "2025-06-18",
        "protocolVersion must be the one synthia-mcp advertises"
    );
    assert_eq!(result["serverInfo"]["name"], "test");

    // `notifications/initialized` is a notification (no id); the
    // server drops it. We send it as a *notification* envelope —
    // no `id` field — to confirm it doesn't error.
    proc.stdin
        .write_all(
            json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized",
                "params": {},
            })
            .to_string()
            .as_bytes(),
        )
        .await
        .unwrap();
    proc.stdin.write_all(b"\n").await.unwrap();
    proc.stdin.flush().await.unwrap();

    let list = proc.request("tools/list", json!({})).await;
    let tools = list["result"]["tools"].as_array().expect("tools array");
    let names: Vec<&str> =
        tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    for expected in [
        "read",
        "write",
        "shell",
        "TodoWrite",
        "web_fetch",
        "schedule",
        "search",
        "synthia",
    ] {
        assert!(
            names.contains(&expected),
            "missing `{expected}` in {names:?}"
        );
    }

    proc.shutdown().await;
}

#[tokio::test]
async fn read_tool_round_trips_through_stdio() {
    let workspace = tempfile::tempdir().unwrap();
    let path = workspace.path().join("hello.txt");
    tokio::fs::write(&path, "first line\nsecond line\n")
        .await
        .unwrap();

    let mut proc = Process::spawn(&workspace).await;
    proc.request(
        "initialize",
        json!({
            "protocolVersion": "2025-06-18",
            "clientInfo": {"name": "test", "version": "0"},
            "capabilities": {},
        }),
    )
    .await;

    let response = proc
        .request(
            "tools/call",
            json!({
                "name": "read",
                "arguments": {
                    "file_path": path.to_string_lossy(),
                    "offset": 1,
                    "limit": 1,
                },
            }),
        )
        .await;
    let result = response.get("result").expect("read returns a result");
    assert_eq!(
        result["isError"],
        json!(false),
        "read of an existing file is not an error"
    );
    let text = result["content"][0]["text"].as_str().expect("text content");
    assert!(text.contains("first line"), "got: {text}");
    assert!(!text.contains("second line"), "got: {text}");

    proc.shutdown().await;
}

#[tokio::test]
async fn write_tool_creates_files_under_the_workspace() {
    let workspace = tempfile::tempdir().unwrap();
    let path = workspace.path().join("created.txt");

    let mut proc = Process::spawn(&workspace).await;
    proc.request(
        "initialize",
        json!({
            "protocolVersion": "2025-06-18",
            "clientInfo": {"name": "test", "version": "0"},
            "capabilities": {},
        }),
    )
    .await;

    let response = proc
        .request(
            "tools/call",
            json!({
                "name": "write",
                "arguments": {
                    "file_path": path.to_string_lossy(),
                    "content": "hello from mcp stdio\n",
                },
            }),
        )
        .await;
    let result = response.get("result").expect("write returns a result");
    assert_eq!(result["isError"], json!(false));
    let text = result["content"][0]["text"].as_str().unwrap();
    // The `write` tool's plugin-brick confirmation is the file path
    // it created; the actual bytes land on disk and are read back
    // by the assertion that follows.
    assert!(
        text.contains("created.txt")
            || text.contains(path.to_string_lossy().as_ref()),
        "got: {text}"
    );
    let on_disk = tokio::fs::read_to_string(&path).await.unwrap();
    assert_eq!(on_disk, "hello from mcp stdio\n");

    proc.shutdown().await;
}

#[tokio::test]
async fn synthia_overview_tool_lists_every_exposed_tool() {
    let workspace = tempfile::tempdir().unwrap();
    let mut proc = Process::spawn(&workspace).await;
    proc.request(
        "initialize",
        json!({
            "protocolVersion": "2025-06-18",
            "clientInfo": {"name": "test", "version": "0"},
            "capabilities": {},
        }),
    )
    .await;

    let response = proc
        .request("tools/call", json!({"name": "synthia", "arguments": {}}))
        .await;
    let result = response.get("result").expect("synthia returns a result");
    let text = result["content"][0]["text"].as_str().expect("text content");
    let parsed: Value = serde_json::from_str(text).unwrap();
    let names: Vec<&str> = parsed["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    for expected in [
        "read",
        "write",
        "shell",
        "TodoWrite",
        "web_fetch",
        "schedule",
        "search",
        "synthia",
    ] {
        assert!(
            names.contains(&expected),
            "overview missing `{expected}`: {names:?}"
        );
    }

    proc.shutdown().await;
}

#[tokio::test]
async fn tools_call_with_unknown_tool_returns_error_envelope() {
    let workspace = tempfile::tempdir().unwrap();
    let mut proc = Process::spawn(&workspace).await;
    proc.request(
        "initialize",
        json!({
            "protocolVersion": "2025-06-18",
            "clientInfo": {"name": "test", "version": "0"},
            "capabilities": {},
        }),
    )
    .await;

    let response = proc
        .request("tools/call", json!({"name": "nope", "arguments": {}}))
        .await;
    let error = response.get("error").expect("unknown tool returns error");
    assert_eq!(error["code"], -32602);
    let message = error["message"].as_str().unwrap();
    assert!(message.contains("unknown tool"), "{message}");

    proc.shutdown().await;
}
