//! `OutputBound` trait — tool output truncation and size caps (PR-6.1).
//!
//! Mirrors opencode's `outputBound.ts`. Each tool output passes through
//! `bind()` which enforces configurable size/line caps. Default: 50 KiB
//! or 2000 lines, whichever is hit first.

use std::sync::Arc;

/// Default maximum content size in bytes (50 KiB).
pub const DEFAULT_MAX_BYTES: usize = 50 * 1024;

/// Default maximum number of lines.
pub const DEFAULT_MAX_LINES: usize = 2000;

/// Configuration for output bounds.
#[derive(Debug, Clone)]
pub struct OutputBoundConfig {
    /// Maximum content size in bytes.
    pub max_bytes: usize,
    /// Maximum number of lines.
    pub max_lines: usize,
}

impl Default for OutputBoundConfig {
    fn default() -> Self {
        Self {
            max_bytes: DEFAULT_MAX_BYTES,
            max_lines: DEFAULT_MAX_LINES,
        }
    }
}

/// Result of binding tool output.
#[derive(Debug, Clone)]
pub struct BoundOutput {
    /// The (possibly truncated) content.
    pub content: Vec<u8>,
    /// Whether the output was truncated.
    pub truncated: bool,
    /// Original content length in bytes.
    pub original_len: usize,
    /// Original line count.
    pub original_lines: usize,
}

/// A trait for enforcing output bounds on tool results.
///
/// Call `bind()` to truncate output that exceeds the configured
/// byte or line limits. The `cleanup()` method provides a hook
/// for clearing retained output data.
pub trait OutputBound: Send + Sync + 'static {
    /// Bind (truncate) the output to the configured limits.
    ///
    /// Returns a `BoundOutput` containing the possibly-truncated
    /// content and metadata about the truncation.
    fn bind(&self, output: Vec<u8>) -> BoundOutput;

    /// Returns the current content length (post-bind).
    fn content_len(&self) -> usize;

    /// Clean up retained output data.
    fn cleanup(&self);
}

/// Default implementation of `OutputBound` with byte and line caps.
pub struct DefaultOutputBound {
    config: OutputBoundConfig,
    retained_len: Arc<std::sync::atomic::AtomicUsize>,
}

impl DefaultOutputBound {
    /// Create a new `DefaultOutputBound` with the given config.
    #[must_use]
    pub fn new(config: OutputBoundConfig) -> Self {
        Self {
            config,
            retained_len: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    /// Create with default limits (50 KiB / 2000 lines).
    #[must_use]
    pub fn default_limits() -> Self {
        Self::new(OutputBoundConfig::default())
    }

    /// Truncate bytes to `max_bytes`.
    fn truncate_bytes(&self, content: &[u8]) -> Vec<u8> {
        if content.len() <= self.config.max_bytes {
            content.to_vec()
        } else {
            content[..self.config.max_bytes].to_vec()
        }
    }

    /// Truncate to `max_lines`.
    fn truncate_lines(&self, content: &[u8]) -> (Vec<u8>, usize, bool) {
        let text = String::from_utf8_lossy(content);
        let lines: Vec<&str> = text.lines().collect();
        let line_count = lines.len();
        if line_count <= self.config.max_lines {
            (content.to_vec(), line_count, false)
        } else {
            let truncated: String = lines[..self.config.max_lines].join("\n");
            (truncated.into_bytes(), line_count, true)
        }
    }
}

impl Default for DefaultOutputBound {
    fn default() -> Self {
        Self::default_limits()
    }
}

impl OutputBound for DefaultOutputBound {
    fn bind(&self, output: Vec<u8>) -> BoundOutput {
        let original_len = output.len();
        let (after_lines, original_lines, lines_truncated) =
            self.truncate_lines(&output);
        let content = self.truncate_bytes(&after_lines);
        let truncated = lines_truncated || content.len() < original_len;

        self.retained_len
            .store(content.len(), std::sync::atomic::Ordering::SeqCst);

        BoundOutput {
            content,
            truncated,
            original_len,
            original_lines,
        }
    }

    fn content_len(&self) -> usize {
        self.retained_len.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn cleanup(&self) {
        self.retained_len
            .store(0, std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bind_under_limits_no_truncation() {
        let ob = DefaultOutputBound::default_limits();
        let output = b"hello world".to_vec();
        let bound = ob.bind(output.clone());
        assert!(!bound.truncated);
        assert_eq!(bound.content, output);
    }

    #[test]
    fn bind_exceeds_byte_limit_truncates() {
        let config = OutputBoundConfig {
            max_bytes: 10,
            max_lines: 9999,
        };
        let ob = DefaultOutputBound::new(config);
        let output = vec![b'x'; 100];
        let bound = ob.bind(output);
        assert!(bound.truncated);
        assert_eq!(bound.content.len(), 10);
        assert_eq!(bound.original_len, 100);
    }

    #[test]
    fn bind_exceeds_line_limit_truncates() {
        let config = OutputBoundConfig {
            max_bytes: 99999,
            max_lines: 2,
        };
        let ob = DefaultOutputBound::new(config);
        let output = b"line1\nline2\nline3\nline4".to_vec();
        let bound = ob.bind(output);
        assert!(bound.truncated);
        assert_eq!(bound.original_lines, 4);
        let text = String::from_utf8(bound.content).unwrap();
        assert_eq!(text.lines().count(), 2);
    }

    #[test]
    fn content_len_updated_after_bind() {
        let ob = DefaultOutputBound::default_limits();
        assert_eq!(ob.content_len(), 0);
        ob.bind(b"hello".to_vec());
        assert_eq!(ob.content_len(), 5);
    }

    #[test]
    fn cleanup_resets_content_len() {
        let ob = DefaultOutputBound::default_limits();
        ob.bind(b"hello".to_vec());
        assert_eq!(ob.content_len(), 5);
        ob.cleanup();
        assert_eq!(ob.content_len(), 0);
    }

    #[test]
    fn default_config_matches_spec() {
        let config = OutputBoundConfig::default();
        assert_eq!(config.max_bytes, 50 * 1024);
        assert_eq!(config.max_lines, 2000);
    }
}
