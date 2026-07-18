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
