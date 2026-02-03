//! Worktree isolation module
//!
//! This module provides git worktree-based directory isolation for parallel task execution.
//! Similar to learn-claude-code s12, this enables:
//! - Directory-level isolation using git worktrees
//! - Task binding to worktrees
//! - Lifecycle event tracking
//!
//! ## Architecture
//!
//! - `.worktrees/index.json` - tracks all worktrees
//! - `.worktrees/events.jsonl` - lifecycle events log
//! - Tasks can be bound to worktrees for isolation

mod event;
mod index;
mod tool;

#[cfg(test)]
mod tests;

pub(crate) use event::WorktreeEventBus;
pub(crate) use index::WorktreeManager;
use serde::{Deserialize, Serialize};
pub use tool::register_worktree_tools;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WorktreeEntry {
    pub name: String,
    pub path: String,
    pub branch: String,
    pub task_id: Option<i64>,
    pub status: String,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
}
