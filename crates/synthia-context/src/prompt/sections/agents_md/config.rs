//! The [`AgentsMdConfig`] struct (the master switch, filename list,
//! per-file and total character caps) plus the three default
//! constants. Also owns the private [`DiscoveredFile`] struct that
//! the walk/merge pipeline produces.

use std::path::PathBuf;

/// Default per-file character cap. Mirrors
/// `sections::inject_workspace_file::WORKSPACE_FILE_MAX_CHARS`.
pub const DEFAULT_MAX_CHARS_PER_FILE: usize = 20_000;
/// Default total character cap across all merged files. Roughly 15K
/// tokens, sized to fit within typical context budgets alongside other
/// system-prompt sections.
pub const DEFAULT_MAX_CHARS_TOTAL: usize = 60_000;
/// Default filename to look for at each ancestor directory.
pub const DEFAULT_FILENAME: &str = "AGENTS.md";

/// Configuration for [`super::section::AgentsMdSection`].
#[derive(Debug, Clone)]
pub struct AgentsMdConfig {
    /// Master switch. When `false`,
    /// [`super::section::AgentsMdSection::build`] returns an empty
    /// string regardless of filesystem contents.
    pub enabled: bool,
    /// Filenames to look for at each ancestor level. Order within this
    /// list is the tie-breaker when multiple files exist at the same
    /// ancestor level (first listed wins).
    pub filenames: Vec<String>,
    /// Per-file character cap. Files exceeding this are truncated and a
    /// marker is appended.
    pub max_chars_per_file: usize,
    /// Total character cap across all merged files. When the cumulative
    /// content would exceed this, the walk stops early and a marker is
    /// appended. Because files are appended in farthest-to-closest
    /// order, the closest file (most-specific override) is naturally
    /// favored.
    pub max_chars_total: usize,
}

impl Default for AgentsMdConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            filenames: vec![DEFAULT_FILENAME.to_string()],
            max_chars_per_file: DEFAULT_MAX_CHARS_PER_FILE,
            max_chars_total: DEFAULT_MAX_CHARS_TOTAL,
        }
    }
}

/// One file discovered by the ancestor walk. The content has already
/// been truncated to `max_chars_per_file` and the truncation marker
/// applied (when applicable).
#[derive(Debug, Clone)]
pub(super) struct DiscoveredFile {
    pub(super) path: PathBuf,
    pub(super) content: String,
}

impl DiscoveredFile {
    pub(super) fn char_count(&self) -> usize {
        self.content.chars().count()
    }
}
