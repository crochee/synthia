//! Public data types — [`TruncateConfig`] and
//! [`TruncatedResult`] — and the private `passthrough`
//! constructor used by [`super::truncate_output`].

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Configuration for the truncation service.
#[derive(Debug, Clone)]
pub struct TruncateConfig {
    /// Maximum number of bytes allowed in the truncated output (excluding
    /// the marker line). Inputs strictly larger than this are truncated.
    pub max_bytes: usize,
    /// Maximum number of lines allowed before offloading. Inputs strictly
    /// larger than this are spilled to disk and summarized.
    pub max_lines: usize,
    /// Number of leading lines to keep.
    pub head_lines: usize,
    /// Number of trailing lines to keep.
    pub tail_lines: usize,
    /// Directory into which full content is written when truncated.
    pub temp_dir: PathBuf,
    /// Optional session identifier used to build a deterministic spill path.
    pub session_id: Option<String>,
    /// Optional tool-call identifier used to build a deterministic spill path.
    pub tool_call_id: Option<String>,
}

/// Return the default base directory for offloaded tool output.
///
/// Prefers `$HOME/.synthia/tool-output` and falls back to the system temp
/// directory if the home directory cannot be determined.
pub fn default_tool_output_dir() -> PathBuf {
    home_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(".synthia")
        .join("tool-output")
}

fn home_dir() -> Option<PathBuf> {
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
    }
}

impl Default for TruncateConfig {
    fn default() -> Self {
        Self {
            max_bytes: 50 * 1024,
            max_lines: 2000,
            head_lines: 100,
            tail_lines: 100,
            temp_dir: default_tool_output_dir(),
            session_id: None,
            tool_call_id: None,
        }
    }
}

/// Result of truncating a single piece of content.
///
/// The field naming follows the new contract (`output` / `original_bytes` /
/// `output_bytes`) but `#[serde(alias)]` keeps the legacy
/// `tool_executor::truncate_result` keys (`content` / `original_length` /
/// `truncated_length`) deserializable for one release cycle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TruncatedResult {
    /// The truncated (or original, if small enough) content.
    #[serde(alias = "content")]
    pub output: String,
    /// Byte length of the original input.
    #[serde(alias = "original_length")]
    pub original_bytes: usize,
    /// Byte length of `output` after truncation.
    #[serde(alias = "truncated_length")]
    pub output_bytes: usize,
    /// True iff the input was actually truncated.
    pub truncated: bool,
    /// Absolute path to the on-disk file with the full content, if written.
    /// `None` when input was not truncated or when the disk write failed.
    pub output_path: Option<PathBuf>,
}

impl TruncatedResult {
    /// Build a [`TruncatedResult`] for an input that is small
    /// enough to pass through unchanged (no truncation, no
    /// disk write).
    pub(super) fn passthrough(content: &str) -> Self {
        Self {
            output: content.to_string(),
            original_bytes: content.len(),
            output_bytes: content.len(),
            truncated: false,
            output_path: None,
        }
    }
}
