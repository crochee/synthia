//! OutputBound + OverflowStrategy + SanitizationPolicy.

use std::{path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize};

/// Per-call and per-session output limits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputBound {
    /// Max bytes per tool call (default: 50 KiB).
    pub per_call_max_bytes: usize,
    /// Max lines per tool call (default: 2000).
    pub per_call_max_lines: usize,
    /// Max bytes per session (default: 4 MiB).
    pub per_session_max_bytes: usize,
    /// Directory for spilled output files.
    pub managed_dir: PathBuf,
    /// How to handle overflow.
    pub overflow_strategy: OverflowStrategy,
    /// Retention period for managed files (default: 7d).
    pub retention: Duration,
    /// Cleanup interval for managed files (default: 1h).
    pub cleanup_interval: Duration,
    /// Sanitization policy.
    pub sanitization: SanitizationPolicy,
}

impl Default for OutputBound {
    fn default() -> Self {
        Self {
            per_call_max_bytes: 50 * 1024,
            per_call_max_lines: 2000,
            per_session_max_bytes: 4 * 1024 * 1024,
            managed_dir: PathBuf::from("/tmp/synthia-managed"),
            overflow_strategy: OverflowStrategy::TruncateHeadTail,
            retention: Duration::from_secs(7 * 24 * 3600),
            cleanup_interval: Duration::from_secs(3600),
            sanitization: SanitizationPolicy::StripControlChars,
        }
    }
}

/// Result of applying [`OutputBound::bind`] to tool output.
#[derive(Debug, Clone)]
pub struct BoundResult {
    /// The (possibly truncated) output string.
    pub output: String,
    /// Whether truncation was applied.
    pub truncated: bool,
    /// Original output size in bytes.
    pub original_bytes: usize,
    /// Final output size in bytes.
    pub output_bytes: usize,
}

impl OutputBound {
    /// Apply output bounds to a tool output string.
    ///
    /// Performs sanitization (strip control chars by default) and
    /// truncation when the output exceeds the configured byte or
    /// line limits. Returns a [`BoundResult`] describing what
    /// happened.
    ///
    /// This is a synchronous, in-memory operation — it does not
    /// spill to managed files. For the full async version with file
    /// spill, see [`crate::tool::bound_output::bound_output`].
    pub fn bind(&self, content: &str) -> BoundResult {
        // Sanitize first
        let sanitized = match self.sanitization {
            SanitizationPolicy::StripControlChars => {
                strip_control_chars(content)
            }
            SanitizationPolicy::WrapUntrusted
            | SanitizationPolicy::RedactUrlsMatching => content.to_string(),
        };

        let original_bytes = content.len();
        let total_lines = sanitized.lines().count();

        if sanitized.len() <= self.per_call_max_bytes
            && total_lines <= self.per_call_max_lines
        {
            return BoundResult {
                output: sanitized,
                truncated: false,
                original_bytes,
                output_bytes: original_bytes,
            };
        }

        let truncated = match self.overflow_strategy {
            OverflowStrategy::TruncateHeadTail => truncate_head_tail(
                &sanitized,
                self.per_call_max_bytes,
                self.per_call_max_lines,
            ),
            OverflowStrategy::TruncateHead => {
                truncate_head_only(&sanitized, self.per_call_max_bytes)
            }
            OverflowStrategy::AlwaysSpill => {
                // Same as head-only for the in-memory path;
                // the async `bound_output` handles actual spill.
                truncate_head_only(&sanitized, self.per_call_max_bytes)
            }
        };

        BoundResult {
            output_bytes: truncated.len(),
            original_bytes,
            truncated: true,
            output: truncated,
        }
    }
}

/// Strip ASCII control characters except `\n`, `\r`, `\t`.
fn strip_control_chars(s: &str) -> String {
    s.chars()
        .filter(|&c| !c.is_control() || c == '\n' || c == '\r' || c == '\t')
        .collect()
}

/// Truncate keeping head + tail, with a marker in the middle.
fn truncate_head_tail(
    text: &str,
    max_bytes: usize,
    max_lines: usize,
) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let total_lines = lines.len();

    // 60% head, 40% tail
    let head_lines = ((max_lines as f64) * 0.6) as usize;
    let tail_lines = max_lines - head_lines;

    let head_count = head_lines.min(total_lines);
    let head: String = lines[..head_count].join("\n");
    let tail_start = total_lines.saturating_sub(tail_lines);
    let tail: String = if tail_start > head_count {
        lines[tail_start..].join("\n")
    } else {
        String::new()
    };

    let omitted = total_lines
        .saturating_sub(head_count)
        .saturating_sub(tail_lines.min(total_lines.saturating_sub(head_count)));
    let marker = format!(
        "\n\n--- [Output truncated: {omitted} lines omitted, keeping head+tail] ---\n\n",
    );

    let result = format!("{head}{marker}{tail}");

    // If still over byte limit, truncate the head portion
    if result.len() > max_bytes {
        let mut truncated = result;
        truncate_in_place(&mut truncated, max_bytes);
        truncated
    } else {
        result
    }
}

/// Truncate keeping only the head, with a trailing marker.
fn truncate_head_only(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut s = text.to_string();
    truncate_in_place(&mut s, max_bytes);
    s
}

/// Truncate a string in place to `max_bytes`, respecting char boundaries.
fn truncate_in_place(text: &mut String, max_bytes: usize) {
    if text.len() <= max_bytes {
        return;
    }
    let mut boundary = max_bytes;
    while boundary > 0 && !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    text.truncate(boundary);
    text.push_str("\n... [truncated]");
}

/// How to handle output that exceeds limits.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize,
)]
pub enum OverflowStrategy {
    /// Keep head + tail, truncate middle (default).
    #[default]
    TruncateHeadTail,
    /// Keep head only.
    TruncateHead,
    /// Always spill to file.
    AlwaysSpill,
}

/// Output sanitization policy.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize,
)]
pub enum SanitizationPolicy {
    /// Strip ASCII control chars (except \\n, \\r, \\t).
    #[default]
    StripControlChars,
    /// Wrap untrusted output in isolation tags.
    WrapUntrusted,
    /// Redact URLs matching a pattern.
    RedactUrlsMatching,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small_bound() -> OutputBound {
        OutputBound {
            per_call_max_bytes: 100,
            per_call_max_lines: 10,
            ..OutputBound::default()
        }
    }

    #[test]
    fn bind_output_within_bounds() {
        let ob = small_bound();
        let input = "hello world";
        let result = ob.bind(input);
        assert!(!result.truncated);
        assert_eq!(result.output, input);
        assert_eq!(result.original_bytes, input.len());
        assert_eq!(result.output_bytes, input.len());
    }

    #[test]
    fn bind_output_exceeds_byte_cap() {
        let ob = small_bound();
        let input = "a".repeat(200);
        let result = ob.bind(&input);
        assert!(result.truncated);
        assert!(result.output.len() < input.len());
        assert!(result.output.contains("truncated"));
        assert_eq!(result.original_bytes, 200);
    }

    #[test]
    fn bind_output_exceeds_line_cap() {
        let ob = small_bound();
        let input: String = (0..20).map(|i| format!("line {i}\n")).collect();
        let result = ob.bind(&input);
        assert!(result.truncated);
        assert!(result.output.contains("truncated"));
    }

    #[test]
    fn bind_output_none_passes_through() {
        // When output_bound is None (i.e. not provided), the caller
        // simply uses the original output. This test verifies the
        // "no bound" path by checking that a small output is unchanged.
        let ob = OutputBound::default();
        let input = "small output";
        let result = ob.bind(input);
        assert!(!result.truncated);
        assert_eq!(result.output, input);
    }

    #[test]
    fn bind_default_50kib_cap() {
        let ob = OutputBound::default();
        assert_eq!(ob.per_call_max_bytes, 50 * 1024);
        assert_eq!(ob.per_call_max_lines, 2000);
    }

    #[test]
    fn bind_strips_control_chars() {
        let ob = small_bound();
        let input = "hello\x00world\x07";
        let result = ob.bind(input);
        assert!(!result.truncated);
        assert_eq!(result.output, "helloworld");
    }

    #[test]
    fn bind_head_only_strategy() {
        let mut ob = small_bound();
        ob.overflow_strategy = OverflowStrategy::TruncateHead;
        let input = "a".repeat(200);
        let result = ob.bind(&input);
        assert!(result.truncated);
        assert!(result.output.ends_with("[truncated]"));
    }
}
