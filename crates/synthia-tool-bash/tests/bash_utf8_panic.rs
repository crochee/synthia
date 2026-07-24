//! Regression test: bash_tool::execute_command must not panic when stdout/stderr
//! contain multi-byte UTF-8 characters and the truncation point falls inside one.
//!
//! Background: pre-fix, `s.truncate(self.max_output_length)` would panic with
//! "byte index N is not a char boundary" when the output ended in Chinese /
//! emoji / other multi-byte sequences. The fix wraps truncation in a
//! `cap_to_char_boundary` helper that walks back to the nearest valid boundary.
//!
//! This test exercises the full `execute_command` path through real bash
//! so that a regression in the integration between the helper and the
//! caller is also caught.

#![cfg(unix)]

use std::{path::PathBuf, sync::Arc};

use synthia_sandbox::SandboxAttempt;
use synthia_tool_bash::{
    bash_tool::BashTool,
    command_blacklist::CommandBlacklist,
    command_manager::CommandManager,
};

fn tool_with_small_max(bytes: usize) -> BashTool {
    let sandbox = CommandBlacklist::new(PathBuf::from("/tmp"));
    BashTool::new(Arc::new(CommandManager::new()), sandbox)
        .with_max_output_length(bytes)
}

#[tokio::test]
async fn execute_command_chinese_stdout_does_not_panic() {
    // "你好世界测试字符串" repeated enough to exceed a tiny max_output_length.
    // Each Chinese char is 3 bytes UTF-8. With max=20 we force truncation
    // to land inside a multi-byte character.
    let tool = tool_with_small_max(20);
    let cmd = "printf '你好世界测试字符串abcde'";
    let (stdout, _stderr, _exit, truncated) = tool
        .execute_command(cmd, 5, &SandboxAttempt::None)
        .await
        .expect("ok");

    assert!(truncated, "output should have been truncated");
    // The stdout should still be valid UTF-8 (it is by construction since
    // we built it via String), and the truncation marker must be present.
    assert!(
        stdout.contains("[stdout truncated at 20 bytes]"),
        "expected truncation marker, got: {}",
        stdout
    );
    // Sanity: no panic, and the pre-marker content does not exceed max_output_length bytes.
    let pre = stdout
        .split("\n\n[stdout truncated at 20 bytes]")
        .next()
        .unwrap_or("");
    assert!(
        pre.len() <= 20,
        "pre-marker content should be <= 20 bytes, got {} bytes: {:?}",
        pre.len(),
        pre
    );
}

#[tokio::test]
async fn execute_command_emoji_stderr_does_not_panic() {
    // 4-byte UTF-8 emoji. With max=10 we force truncation inside an emoji.
    let tool = tool_with_small_max(10);
    let cmd = "printf '😀😁😂🤣😃😄😅😆😉' 1>&2; exit 0";
    let (_stdout, stderr, _exit, truncated) = tool
        .execute_command(cmd, 5, &SandboxAttempt::None)
        .await
        .expect("ok");

    assert!(truncated, "stderr should have been truncated");
    assert!(
        stderr.contains("[stderr truncated at 10 bytes]"),
        "expected stderr truncation marker, got: {}",
        stderr
    );
    let pre = stderr
        .split("\n\n[stderr truncated at 10 bytes]")
        .next()
        .unwrap_or("");
    assert!(
        pre.len() <= 10,
        "pre-marker stderr should be <= 10 bytes, got {} bytes: {:?}",
        pre.len(),
        pre
    );
}

#[tokio::test]
async fn execute_command_mixed_multibyte_does_not_panic() {
    // Mix of ASCII + Chinese + emoji. With max=15 the cut point will land
    // inside a multi-byte character almost every time.
    let tool = tool_with_small_max(15);
    let cmd = "printf 'A你好😀BC世界😁DEF'";
    let (stdout, _stderr, _exit, truncated) = tool
        .execute_command(cmd, 5, &SandboxAttempt::None)
        .await
        .expect("ok");

    assert!(truncated);
    assert!(stdout.contains("[stdout truncated at 15 bytes]"));
    // The actual content MUST be valid UTF-8.
    let pre = stdout
        .split("\n\n[stdout truncated at 15 bytes]")
        .next()
        .unwrap_or("");
    assert!(std::str::from_utf8(pre.as_bytes()).is_ok());
    assert!(pre.len() <= 15);
}

#[tokio::test]
async fn execute_command_ascii_unaffected() {
    // Pure ASCII with max=10: behavior must be identical to plain
    // String::truncate — no boundary adjustment needed.
    let tool = tool_with_small_max(10);
    let cmd = "printf 'Hello, World! This is ASCII only.'";
    let (stdout, _stderr, _exit, truncated) = tool
        .execute_command(cmd, 5, &SandboxAttempt::None)
        .await
        .expect("ok");

    assert!(truncated);
    assert!(stdout.contains("[stdout truncated at 10 bytes]"));
    let pre = stdout
        .split("\n\n[stdout truncated at 10 bytes]")
        .next()
        .unwrap_or("");
    // ASCII: pre is exactly 10 bytes.
    assert_eq!(pre.len(), 10);
    assert_eq!(pre, "Hello, Wor");
}

#[tokio::test]
async fn execute_command_short_output_not_truncated() {
    // Output shorter than max → no truncation, flag false, no marker.
    let tool = tool_with_small_max(1000);
    let cmd = "printf 'short'";
    let (stdout, _stderr, _exit, truncated) = tool
        .execute_command(cmd, 5, &SandboxAttempt::None)
        .await
        .expect("ok");

    assert!(!truncated);
    assert!(!stdout.contains("truncated at"));
    assert_eq!(stdout, "short");
}
