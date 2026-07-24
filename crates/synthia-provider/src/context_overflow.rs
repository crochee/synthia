//! Context-overflow detection for provider error messages.
//!
//! Provides:
//! - [`ContextOverflowDetector`] — regex-based classifier that scans
//!   provider API error strings for context-window-overflow signals.
//! - [`is_silent_overflow`] — token-budget probe that flags overflow
//!   when the provider returns no explicit error but the consumed
//!   tokens already exceed the model's context window.
//! - [`synthesize_orphan_result`] — placeholder `ToolResult` body used
//!   to recover a truncated conversation when a tool call has no
//!   matching tool result.
//!
//! Exclusion patterns (rate-limit / throttling) take priority over
//! overflow patterns: a message that mentions both is treated as a
//! rate-limit error, not an overflow.

use std::sync::OnceLock;

use regex::Regex;

use crate::types::TokenUsage;

/// Compiled overflow patterns indexed lazily on first use.
///
/// `std::sync::OnceLock` is used instead of `once_cell::sync::Lazy`
/// because `once_cell` is not a workspace dependency. `OnceLock` is
/// part of std since Rust 1.70 and the workspace MSRV is 1.95.
static OVERFLOW_PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();

/// Compiled exclusion patterns indexed lazily on first use.
static EXCLUSION_PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();

/// Build the 21 provider-specific overflow regexes.
///
/// Patterns are grouped by provider; each group is preceded by a
/// comment naming the source.
fn build_overflow_patterns() -> Vec<Regex> {
    let patterns: &[&str] = &[
        // Anthropic
        r"(?i)context length exceeded",
        r"(?i)prompt is too long",
        r"(?i)input.*too long",
        r"(?i)exceeds.*context window",
        r"(?i)context window.*exceeded",
        // OpenAI
        r"(?i)maximum context length",
        r"(?i)this model.*maximum context",
        r"(?i)reduce.*prompt.*length",
        r"(?i)context.*exceeded",
        r"(?i)token limit exceeded",
        // Google / Gemini
        r"(?i)exceeds.*token.*limit",
        r"(?i)input.*token.*limit",
        r"(?i)request.*too large",
        r"(?i)content.*too long",
        // Other providers — Cohere / Mistral / Together / Anyscale /
        // Replicate / Fireworks / DeepSeek
        r"(?i)input length exceeded",
        r"(?i)prompt.*exceeds",
        r"(?i)too many tokens",
        r"(?i)context.*full",
        r"(?i)sequence length",
        r"(?i)input size.*limit",
        r"(?i)request.*length.*exceeds",
    ];
    patterns
        .iter()
        .map(|p| Regex::new(p).expect("overflow regex must compile"))
        .collect()
}

/// Build the 3 exclusion regexes (rate-limit / throttling).
fn build_exclusion_patterns() -> Vec<Regex> {
    let patterns: &[&str] =
        &[r"(?i)rate limit", r"(?i)too many requests", r"(?i)throttl"];
    patterns
        .iter()
        .map(|p| Regex::new(p).expect("exclusion regex must compile"))
        .collect()
}

fn overflow_patterns() -> &'static [Regex] {
    OVERFLOW_PATTERNS
        .get_or_init(build_overflow_patterns)
        .as_slice()
}

fn exclusion_patterns() -> &'static [Regex] {
    EXCLUSION_PATTERNS
        .get_or_init(build_exclusion_patterns)
        .as_slice()
}

/// Detector classifying provider error strings as context-overflow.
///
/// Stateless — the patterns are process-global statics shared by all
/// callers. Construct cheaply with [`ContextOverflowDetector::new`]
/// (or `Default`) and reuse.
#[derive(Debug, Clone, Default)]
pub struct ContextOverflowDetector;

impl ContextOverflowDetector {
    pub fn new() -> Self {
        Self
    }

    /// Returns `true` if `error_message` signals a context-window
    /// overflow, unless an exclusion pattern (rate limit / throttling)
    /// matches first.
    ///
    /// Exclusion wins: if any exclusion pattern matches, the result is
    /// `false` even when an overflow pattern also matches. This
    /// prevents rate-limit errors (which often contain "exceeded")
    /// from being misclassified as overflow.
    pub fn is_overflow(&self, error_message: &str) -> bool {
        if exclusion_patterns()
            .iter()
            .any(|re| re.is_match(error_message))
        {
            return false;
        }
        overflow_patterns()
            .iter()
            .any(|re| re.is_match(error_message))
    }
}

/// Returns `true` when consumed tokens already exceed `context_window`
/// even though the provider returned no explicit error string.
///
/// Computed as `prompt_tokens + cache_read_tokens.unwrap_or(0)`. This
/// catches the "silent overflow" case where the provider truncated the
/// conversation without emitting a structured error.
pub fn is_silent_overflow(usage: &TokenUsage, context_window: usize) -> bool {
    let consumed = usage.prompt_tokens + usage.cache_read_tokens.unwrap_or(0);
    consumed > context_window
}

/// Placeholder body for an orphan tool result.
///
/// The caller wraps the returned string in a `ToolResult` with the
/// given `tool_use_id`. Kept as a `String` (not a `ToolResult`) to
/// avoid tight coupling with the `ToolResult` type.
pub fn synthesize_orphan_result(_tool_use_id: &str) -> String {
    "[orphan tool call - result truncated]".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ToolResult;

    fn detector() -> ContextOverflowDetector {
        ContextOverflowDetector::new()
    }

    fn usage(prompt: usize, cache_read: Option<usize>) -> TokenUsage {
        TokenUsage {
            prompt_tokens: prompt,
            completion_tokens: 0,
            total_tokens: prompt + cache_read.unwrap_or(0),
            cached_prompt_tokens: None,
            cache_read_tokens: cache_read,
            cache_write_tokens: None,
        }
    }

    #[test]
    fn detects_anthropic_overflow_patterns() {
        let d = detector();
        let messages = [
            "context length exceeded",
            "prompt is too long",
            "input is too long",
            "exceeds the context window",
            "context window has been exceeded",
        ];
        for msg in messages {
            assert!(
                d.is_overflow(msg),
                "expected Anthropic overflow pattern to match: {msg}"
            );
        }
    }

    #[test]
    fn detects_openai_overflow_patterns() {
        let d = detector();
        let messages = [
            "maximum context length exceeded",
            "this model's maximum context length is 8192",
            "please reduce the prompt length",
            "context limit exceeded",
            "token limit exceeded",
        ];
        for msg in messages {
            assert!(
                d.is_overflow(msg),
                "expected OpenAI overflow pattern to match: {msg}"
            );
        }
    }

    #[test]
    fn detects_google_overflow_patterns() {
        let d = detector();
        let messages = [
            "exceeds the token limit",
            "input token limit reached",
            "request is too large",
            "content is too long",
        ];
        for msg in messages {
            assert!(
                d.is_overflow(msg),
                "expected Google/Gemini overflow pattern to match: {msg}"
            );
        }
    }

    #[test]
    fn detects_other_provider_patterns() {
        let d = detector();
        let messages = [
            "input length exceeded",
            "prompt exceeds maximum",
            "too many tokens",
            "context is full",
            "sequence length exceeded",
            "input size limit exceeded",
            "request length exceeds maximum",
        ];
        for msg in messages {
            assert!(
                d.is_overflow(msg),
                "expected other-provider overflow pattern to match: {msg}"
            );
        }
    }

    #[test]
    fn excludes_rate_limit() {
        let d = detector();
        assert!(!d.is_overflow("Rate limit exceeded, retry after 30s"));
    }

    #[test]
    fn excludes_too_many_requests() {
        let d = detector();
        assert!(!d.is_overflow("Too many requests"));
    }

    #[test]
    fn excludes_throttling() {
        let d = detector();
        assert!(!d.is_overflow("Request throttled"));
        assert!(!d.is_overflow("throttling"));
    }

    #[test]
    fn exclusion_overrides_overflow() {
        let d = detector();
        // Overflow pattern matches ("context length exceeded") but
        // exclusion pattern ("rate limit") also matches -> exclusion wins.
        assert!(!d.is_overflow("context length exceeded but rate limit"));
    }

    #[test]
    fn detects_silent_overflow() {
        let u = usage(50_000, Some(80_000));
        assert!(is_silent_overflow(&u, 100_000));
    }

    #[test]
    fn no_silent_overflow_under_limit() {
        let u = usage(50_000, Some(80_000));
        assert!(!is_silent_overflow(&u, 200_000));
    }

    #[test]
    fn silent_overflow_handles_none_cache_read() {
        let u = usage(150_000, None);
        assert!(is_silent_overflow(&u, 100_000));
    }

    #[test]
    fn orphan_result_has_correct_text() {
        let body = synthesize_orphan_result("toolu_123");
        assert!(body.contains("[orphan tool call - result truncated]"));
    }

    #[test]
    fn orphan_result_includes_tool_use_id_in_caller() {
        let body = synthesize_orphan_result("toolu_123");
        let result = ToolResult::new("toolu_123", body);
        assert_eq!(result.tool_use_id, "toolu_123");
    }
}
