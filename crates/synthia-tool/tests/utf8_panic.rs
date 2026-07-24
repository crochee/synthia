//! End-to-end UTF-8 safe truncation tests for `synthia-tool`.
//!
//! These tests cover the wire-up between the public
//! `cap_to_char_boundary` helper (in `builtin/utf8_safe.rs`) and the
//! call sites that use it, across multiple tool surfaces. The
//! per-helper unit tests live in `utf8_safe.rs`; this file exercises
//! the *integrated* behavior so that a regression in either the
//! helper or its caller is caught.
//!
//! Background: pre-fix, callers used `String::truncate(usize)` directly,
//! which panics with "byte index N is not a char boundary" when the
//! truncation point lands inside a multi-byte UTF-8 character. The
//! fix wraps every such call in `cap_to_char_boundary`, which walks
//! back to the nearest valid boundary.
//!
//! (d) and (e) — BashTool output with Chinese / emoji — are covered
//! in `synthia-tool-bash/tests/bash_utf8_panic.rs` because BashTool
//! lives in that crate. Importing it from here would create a
//! circular dependency.

#![cfg(unix)]

use std::path::PathBuf;

use synthia_tool::{
    Tool,
    ToolInput,
    builtin::WebFetchTool,
    types::ToolExecutionContext,
};

fn make_input(name: &str, args: serde_json::Value) -> ToolInput {
    ToolInput {
        name: name.to_string(),
        input: args,
        context: ToolExecutionContext::new(
            "test-session".to_string(),
            PathBuf::from("/tmp"),
        ),
    }
}

// ===== (a) WebFetchTool with Chinese response body =====

#[test]
fn web_truncate_cjk_does_not_panic() {
    // "你好世界这是一个测试" repeated to exceed max_len. Each char is
    // 3 bytes UTF-8, so a 20-byte cap forces truncation to land
    // inside a multi-byte character.
    let body: String = "你好世界这是一个测试".repeat(20);
    assert!(body.len() > 30);

    let (truncated, was_truncated) =
        WebFetchTool::truncate_response_body(body, 20);

    assert!(was_truncated, "body should have been truncated");
    assert!(
        truncated.contains("[Response truncated at 20 bytes]"),
        "expected truncation marker, got: {truncated}"
    );
    // Pre-marker content must be valid UTF-8 and not exceed the cap.
    let pre = truncated
        .split("\n\n[Response truncated at 20 bytes]")
        .next()
        .unwrap_or("");
    assert!(
        std::str::from_utf8(pre.as_bytes()).is_ok(),
        "pre-marker content is not valid UTF-8: {:?}",
        pre
    );
    assert!(
        pre.len() <= 20,
        "pre-marker content should be <= 20 bytes, got {} bytes: {:?}",
        pre.len(),
        pre
    );
}

// ===== (b) WebFetchTool with emoji response body =====

#[test]
fn web_truncate_emoji_does_not_panic() {
    // 4-byte UTF-8 emoji. Cap at 10 bytes forces truncation inside
    // a multi-byte sequence.
    let body: String = "😀😁😂🤣😃😄😅😆😉😊😋😎😍😀".to_string();
    assert!(body.len() > 10);

    let (truncated, was_truncated) =
        WebFetchTool::truncate_response_body(body, 10);

    assert!(was_truncated);
    assert!(truncated.contains("[Response truncated at 10 bytes]"));
    let pre = truncated
        .split("\n\n[Response truncated at 10 bytes]")
        .next()
        .unwrap_or("");
    assert!(std::str::from_utf8(pre.as_bytes()).is_ok());
    assert!(pre.len() <= 10, "pre.len() = {}", pre.len());
}

// ===== (c) GrepTool searches Chinese files without panic =====

#[tokio::test]
async fn grep_chinese_file_does_not_panic() {
    use synthia_tool::builtin::GrepTool;

    let dir = tempfile::tempdir().unwrap();
    // Mix of ASCII and CJK content; GrepTool must walk the file,
    // match the regex, and produce `path:line:> content` lines
    // without panicking on the multi-byte bytes.
    std::fs::write(
        dir.path().join("chinese.md"),
        "第一行 hello world\n第二行 foo bar\n第三行 测试 中文\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("ascii.txt"), "plain ASCII content\n")
        .unwrap();

    let tool = GrepTool;
    let output = tool
        .call(make_input(
            "grep",
            serde_json::json!({
                "pattern": "测试",
                "path": dir.path().to_str().unwrap(),
            }),
        ))
        .await;

    let text = output.content.iter().find_map(|c| c.text()).unwrap();
    assert!(
        text.contains("测试"),
        "expected match line containing 测试, got: {text}"
    );
    // ASCII file should not show up.
    assert!(
        !text.contains("plain ASCII"),
        "ASCII file should not match 测试 pattern, got: {text}"
    );
}

// ===== (d) (e) BashTool cases are covered in synthia-tool-bash =====
//
// Reference: `crates/synthia-tool-bash/tests/bash_utf8_panic.rs` covers:
//   - execute_command_chinese_stdout_does_not_panic
//   - execute_command_emoji_stderr_does_not_panic
//   - execute_command_mixed_multibyte_does_not_panic
//   - execute_command_ascii_unaffected
//   - execute_command_short_output_not_truncated
//
// These are intentionally not duplicated here to avoid the
// synthia-tool ↔ synthia-tool-bash circular dependency.
