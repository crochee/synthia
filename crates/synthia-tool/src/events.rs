//! Progress events emitted by file-mutating tools.
//!
//! These events are produced during tool execution (e.g. when a patch hunk
//! is applied) and are forwarded by the orchestrator's [`ToolAdapter`] as
//! [`synthia_tool_orchestrator::ToolOrchestratorEvent::FileChange`] events.

use serde::{Deserialize, Serialize};

/// A single file change event emitted while a tool is mutating files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileChangeEvent {
    /// A new file was created.
    FileAdded {
        /// Absolute or workspace-relative path that was added.
        path: String,
    },
    /// An existing file was modified.
    FileUpdated {
        /// Absolute or workspace-relative path that was updated.
        path: String,
    },
    /// A file was deleted.
    FileDeleted {
        /// Absolute or workspace-relative path that was deleted.
        path: String,
    },
    /// A hunk inside an update operation was applied.
    HunkApplied {
        /// Path of the file being updated.
        path: String,
        /// Zero-based index of the hunk within the update operation.
        hunk_index: usize,
    },
}

impl FileChangeEvent {
    /// Return the path associated with the event.
    pub fn path(&self) -> &str {
        match self {
            Self::FileAdded { path }
            | Self::FileUpdated { path }
            | Self::FileDeleted { path }
            | Self::HunkApplied { path, .. } => path,
        }
    }
}
