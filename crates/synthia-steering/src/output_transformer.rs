//! [`OutputTransformer`] — async post-processing of tool outputs
//! before they are committed to the conversation.
//!
//! Runs in the commit path *after* a tool finished and *before*
//! the result is appended to the message list, keyed by tool name
//! so a transformer can downsample verbose tools (a 2 MB
//! `web_fetch` body) differently from terse ones. Error outputs
//! are never transformed — corrective feedback must reach the
//! model verbatim.

use async_trait::async_trait;
use synthia_context::AgentState;

/// Per-tool output post-processor.
#[async_trait]
pub trait OutputTransformer: Send + Sync {
    /// Rewrite `output` (the tool's final content) for the given
    /// tool and return the string to commit.
    async fn transform(
        &self,
        output: String,
        tool_name: &str,
        state: &AgentState,
    ) -> String;

    /// Cheap token estimate of an output. Shared
    /// 4-chars-≈-1-token heuristic (same as the context crate's
    /// message estimator).
    fn estimate_output_tokens(&self, output: &str) -> usize {
        output.len() / 4 + 1
    }
}

/// Identity transformer — returns outputs untouched. The neutral
/// element so callers never hold an `Option<OutputTransformer>`.
pub struct NoopOutputTransformer;

#[async_trait]
impl OutputTransformer for NoopOutputTransformer {
    async fn transform(
        &self,
        output: String,
        _tool_name: &str,
        _state: &AgentState,
    ) -> String {
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shared estimate heuristic MUST be ~len/4.
    #[test]
    fn estimate_follows_four_chars_per_token() {
        let t = NoopOutputTransformer;
        assert_eq!(t.estimate_output_tokens(""), 1);
        assert_eq!(t.estimate_output_tokens("abcd"), 2);
        assert_eq!(t.estimate_output_tokens(&"x".repeat(400)), 101);
    }

    /// The identity transformer MUST hand the string straight
    /// back.
    #[tokio::test]
    async fn identity_transform_returns_input() {
        let t = NoopOutputTransformer;
        let out = t
            .transform(
                "hello".to_string(),
                "shell",
                &AgentState::with_window(100),
            )
            .await;
        assert_eq!(out, "hello");
    }
}
