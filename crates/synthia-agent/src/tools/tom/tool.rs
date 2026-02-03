//! ContextInject tool implementation
//!
//! This module provides the ContextInject tool for injecting custom context
//! through environment variables.

use async_trait::async_trait;
use rmcp::model::CallToolResult;
use serde_json::Value;

use crate::tools::Tool;

/// Maximum bytes allowed
const MAX_BYTES: usize = 65_536;

/// Tool for injecting custom context from environment variables into the conversation.
/// Use this when you need to add additional instructions, project context, or reference
/// material that was configured by the user via environment variables.
#[derive(Debug, Clone, Copy)]
pub struct ContextInjectTool;

impl ContextInjectTool {
    pub fn new() -> Self {
        Self
    }

    /// Get context from environment variables
    pub(crate) async fn get_moim(&self) -> Option<String> {
        let mut parts = Vec::new();

        // Check for direct text injection
        if let Ok(text) = std::env::var("MOIM_MESSAGE_TEXT")
            && !text.trim().is_empty()
        {
            parts.push(ContextInjectTool::truncate_utf8(text));
        }

        // Check for file-based injection
        if let Ok(path) = std::env::var("MOIM_MESSAGE_FILE") {
            let expanded = ContextInjectTool::expand_tilde(&path);
            if let Some(content) =
                ContextInjectTool::read_bounded(&expanded).await
                && !content.trim().is_empty()
            {
                parts.push(content);
            }
        }

        if parts.is_empty() {
            None
        } else {
            Some(parts.join("\n"))
        }
    }

    /// Expand tilde in path
    fn expand_tilde(path: &str) -> String {
        if path.starts_with("~/")
            && let Some(home) = dirs::home_dir()
        {
            return path.replacen("~", &home.to_string_lossy(), 1);
        }
        path.to_string()
    }

    /// Read file with bounded size
    async fn read_bounded(path: &str) -> Option<String> {
        use tokio::io::AsyncReadExt;

        let mut file = tokio::fs::File::open(path).await.ok()?;
        let mut buf = vec![0u8; MAX_BYTES];
        let mut total = 0;

        loop {
            let n = file.read(&mut buf[total..]).await.ok()?;
            if n == 0 {
                break;
            }
            total += n;
            if total >= MAX_BYTES {
                break;
            }
        }

        buf.truncate(total);
        let s = String::from_utf8_lossy(&buf).into_owned();
        Some(Self::truncate_utf8(s))
    }

    /// Truncate UTF-8 string to max bytes while preserving valid UTF-8
    fn truncate_utf8(s: String) -> String {
        if s.len() <= MAX_BYTES {
            return s;
        }

        s.char_indices()
            .take_while(|(i, c)| i + c.len_utf8() <= MAX_BYTES)
            .map(|(_, c)| c)
            .collect()
    }
}

impl Default for ContextInjectTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ContextInjectTool {
    fn name(&self) -> &str {
        "ContextInject"
    }

    fn description(&self) -> &str {
        "Inject context from env vars (max 65KB)."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn call(&self, _args: Value) -> CallToolResult {
        match self.get_moim().await {
            Some(context) => {
                CallToolResult::success(vec![rmcp::model::Content::text(
                    context,
                )])
            }
            None => {
                CallToolResult::success(vec![rmcp::model::Content::text(
                    "No context configured. Set MOIM_MESSAGE_TEXT or MOIM_MESSAGE_FILE environment variables.".to_string(),
                )])
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ContextInjectTool creation tests

    #[test]
    fn test_context_inject_tool_creation() {
        let tool = ContextInjectTool::new();
        assert_eq!(tool.name(), "ContextInject");
        assert!(!tool.description().is_empty());
    }

    #[test]
    fn test_context_inject_tool_default() {
        let tool = ContextInjectTool;
        assert_eq!(tool.name(), "ContextInject");
    }

    #[test]
    fn test_context_inject_tool_name() {
        let tool = ContextInjectTool::new();
        assert_eq!(tool.name(), "ContextInject");
    }

    #[test]
    fn test_context_inject_tool_description() {
        let tool = ContextInjectTool::new();
        let desc = tool.description();
        assert!(!desc.is_empty());
        assert!(desc.contains("65KB") || desc.contains("context"));
    }

    #[test]
    fn test_context_inject_tool_parameters() {
        let tool = ContextInjectTool::new();
        let params = tool.parameters();
        assert!(params.is_object());
        let obj = params.as_object().unwrap();
        assert_eq!(obj.get("type").unwrap(), "object");
    }

    // truncate_utf8 tests

    #[test]
    fn test_truncate_utf8_short_string() {
        let short = "Hello, world!".to_string();
        assert_eq!(ContextInjectTool::truncate_utf8(short.clone()), short);
    }

    #[test]
    fn test_truncate_utf8_exact_boundary() {
        let s = "a".repeat(MAX_BYTES);
        assert_eq!(ContextInjectTool::truncate_utf8(s).len(), MAX_BYTES);
    }

    #[test]
    fn test_truncate_utf8_just_over_boundary() {
        let s = "a".repeat(MAX_BYTES + 1);
        let truncated = ContextInjectTool::truncate_utf8(s);
        assert!(truncated.len() <= MAX_BYTES);
    }

    #[test]
    fn test_truncate_utf8_long_string() {
        let long = "x".repeat(100_000);
        let truncated = ContextInjectTool::truncate_utf8(long);
        assert!(truncated.len() <= MAX_BYTES);
        assert!(truncated.chars().all(|c| c == 'x'));
    }

    #[test]
    fn test_truncate_utf8_preserves_valid_utf8() {
        let emoji = "🎉".repeat(50_000);
        let truncated = ContextInjectTool::truncate_utf8(emoji);
        assert!(truncated.len() <= MAX_BYTES);
        assert!(truncated.chars().all(|c| c == '🎉'));
    }

    #[test]
    fn test_truncate_utf8_mixed_ascii_and_unicode() {
        // Mix of ASCII and multi-byte characters
        let mixed = "abc🎉def🎊ghi".repeat(10_000);
        let truncated = ContextInjectTool::truncate_utf8(mixed);
        assert!(truncated.len() <= MAX_BYTES);
        // Verify it's valid UTF-8 by converting back to string
        assert!(std::str::from_utf8(truncated.as_bytes()).is_ok());
    }

    #[test]
    fn test_truncate_utf8_empty_string() {
        let empty = String::new();
        assert_eq!(ContextInjectTool::truncate_utf8(empty.clone()), empty);
    }

    #[test]
    fn test_truncate_utf8_single_char_at_boundary() {
        // Test truncation at a single character boundary
        let s = "🎉".repeat(10_000);
        let truncated = ContextInjectTool::truncate_utf8(s);
        // Should not panic and result should be valid
        assert!(truncated.len() <= MAX_BYTES);
    }

    // expand_tilde tests

    #[test]
    fn test_expand_tilde_regular_path() {
        let path = "/usr/local/bin";
        let expanded = ContextInjectTool::expand_tilde(path);
        assert_eq!(expanded, "/usr/local/bin");
    }

    #[test]
    fn test_expand_tilde_path_without_tilde() {
        let path = "/home/user/file.txt";
        let expanded = ContextInjectTool::expand_tilde(path);
        assert_eq!(expanded, "/home/user/file.txt");
    }

    #[test]
    fn test_expand_tilde_empty_path() {
        let path = "";
        let expanded = ContextInjectTool::expand_tilde(path);
        assert_eq!(expanded, "");
    }

    // read_bounded tests

    #[tokio::test]
    async fn test_read_bounded_nonexistent_file() {
        let result =
            ContextInjectTool::read_bounded("/nonexistent/path/to/file.txt")
                .await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_read_bounded_empty_file() {
        use tokio::io::AsyncWriteExt;

        let temp_dir = std::env::temp_dir();
        let temp_path = temp_dir.join("test_read_bounded_empty.txt");

        let mut file = tokio::fs::File::create(&temp_path).await.unwrap();
        file.write_all(b"").await.unwrap();

        let result =
            ContextInjectTool::read_bounded(temp_path.to_str().unwrap()).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "");

        tokio::fs::remove_file(&temp_path).await.ok();
    }

    #[tokio::test]
    async fn test_read_bounded_small_file() {
        use tokio::io::AsyncWriteExt;

        let temp_dir = std::env::temp_dir();
        let temp_path = temp_dir.join("test_read_bounded_small.txt");

        let mut file = tokio::fs::File::create(&temp_path).await.unwrap();
        file.write_all(b"Hello, world!").await.unwrap();

        let result =
            ContextInjectTool::read_bounded(temp_path.to_str().unwrap()).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "Hello, world!");

        tokio::fs::remove_file(&temp_path).await.ok();
    }

    #[tokio::test]
    async fn test_read_bounded_large_file() {
        use tokio::io::AsyncWriteExt;

        let temp_dir = std::env::temp_dir();
        let temp_path = temp_dir.join("test_read_bounded_large.txt");

        let large_content = "x".repeat(100_000);
        let mut file = tokio::fs::File::create(&temp_path).await.unwrap();
        file.write_all(large_content.as_bytes()).await.unwrap();

        // Should truncate to MAX_BYTES
        let result =
            ContextInjectTool::read_bounded(temp_path.to_str().unwrap()).await;
        assert!(result.is_some());
        assert!(result.unwrap().len() <= MAX_BYTES);

        tokio::fs::remove_file(&temp_path).await.ok();
    }

    #[tokio::test]
    async fn test_read_bounded_binary_content() {
        use tokio::io::AsyncWriteExt;

        let temp_dir = std::env::temp_dir();
        let temp_path = temp_dir.join("test_read_bounded_binary.txt");

        let binary_content: Vec<u8> = (0..255).collect();
        let mut file = tokio::fs::File::create(&temp_path).await.unwrap();
        file.write_all(&binary_content).await.unwrap();

        let result =
            ContextInjectTool::read_bounded(temp_path.to_str().unwrap()).await;
        assert!(result.is_some());
        let content = result.unwrap();
        assert!(content.len() <= MAX_BYTES);

        tokio::fs::remove_file(&temp_path).await.ok();
    }

    // context_inject_tool_call tests

    #[tokio::test]
    async fn test_context_inject_tool_call_no_env() {
        // Ensure env vars are not set
        // Safety: env var operations are unsafe in tests but required for functionality
        unsafe {
            std::env::remove_var("MOIM_MESSAGE_TEXT");
            std::env::remove_var("MOIM_MESSAGE_FILE");
        }

        let tool = ContextInjectTool::new();
        let args = serde_json::json!({});

        let result = tool.call(args).await;
        assert!(result.is_error.is_none() || result.is_error == Some(false));
        assert!(!result.content.is_empty());
    }

    #[tokio::test]
    async fn test_context_inject_tool_call_with_text_env() {
        use tokio::io::AsyncWriteExt;

        let temp_dir = std::env::temp_dir();
        let temp_path = temp_dir.join("test_moim_text.txt");

        let content = "Test context from file";
        let mut file = tokio::fs::File::create(&temp_path).await.unwrap();
        file.write_all(content.as_bytes()).await.unwrap();

        // Safety: env var operations are unsafe in tests but required for functionality
        unsafe {
            std::env::set_var("MOIM_MESSAGE_TEXT", content);
            std::env::remove_var("MOIM_MESSAGE_FILE");
        }

        let tool = ContextInjectTool::new();
        let result = tool.call(serde_json::json!({})).await;

        assert!(result.is_error.is_none() || result.is_error == Some(false));

        unsafe { std::env::remove_var("MOIM_MESSAGE_TEXT") };
        tokio::fs::remove_file(&temp_path).await.ok();
    }

    #[tokio::test]
    async fn test_context_inject_tool_call_with_file_env() {
        use tokio::io::AsyncWriteExt;

        let temp_dir = std::env::temp_dir();
        let temp_path = temp_dir.join("test_moim_file.txt");

        let content = "Test context from file";
        let mut file = tokio::fs::File::create(&temp_path).await.unwrap();
        file.write_all(content.as_bytes()).await.unwrap();

        unsafe {
            std::env::remove_var("MOIM_MESSAGE_TEXT");
            std::env::set_var("MOIM_MESSAGE_FILE", temp_path.to_str().unwrap());
        }

        let tool = ContextInjectTool::new();
        let result = tool.call(serde_json::json!({})).await;

        assert!(result.is_error.is_none() || result.is_error == Some(false));

        unsafe { std::env::remove_var("MOIM_MESSAGE_FILE") };
        tokio::fs::remove_file(&temp_path).await.ok();
    }

    #[tokio::test]
    async fn test_context_inject_tool_call_empty_text() {
        unsafe {
            std::env::set_var("MOIM_MESSAGE_TEXT", "");
            std::env::remove_var("MOIM_MESSAGE_FILE");
        }

        let tool = ContextInjectTool::new();
        let result = tool.call(serde_json::json!({})).await;

        assert!(result.is_error.is_none() || result.is_error == Some(false));

        unsafe { std::env::remove_var("MOIM_MESSAGE_TEXT") };
    }

    #[tokio::test]
    async fn test_context_inject_tool_call_nonexistent_file() {
        unsafe {
            std::env::remove_var("MOIM_MESSAGE_TEXT");
            std::env::set_var(
                "MOIM_MESSAGE_FILE",
                "/nonexistent/path/to/file.txt",
            );
        }

        let tool = ContextInjectTool::new();
        let result = tool.call(serde_json::json!({})).await;

        // Should still succeed (gracefully handles missing file)
        assert!(result.is_error.is_none() || result.is_error == Some(false));

        unsafe { std::env::remove_var("MOIM_MESSAGE_FILE") };
    }

    // MAX_BYTES constant tests

    #[test]
    fn test_max_bytes_value() {
        assert_eq!(MAX_BYTES, 65_536);
    }

    #[test]
    fn test_max_bytes_is_reasonable() {
        // Verify MAX_BYTES is a reasonable size (between 1KB and 1MB)
        const { assert!(MAX_BYTES >= 1024) };
        const { assert!(MAX_BYTES <= 1_048_576) };
    }
}
