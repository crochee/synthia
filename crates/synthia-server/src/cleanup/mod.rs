//! Resource cleanup daemon that runs as a background Tokio task.
//!
//! Periodically scans and removes:
//! - Completed sessions older than the configured retention period
//! - Temporary files in `.agents/tmp/` older than the configured TTL
//! - Excess checkpoints per session (max 5)
//! - Orphaned checkpoints (checkpoints for deleted sessions)
//!
//! Submodule layout:
//!
//! - [`types`]: the public [`CleanupConfig`] struct +
//!   `Default` impl, the public [`CleanupMetrics`] struct
//!   + `Default` impl, and the
//!     `AddAssign<&Self> for CleanupMetrics` impl that the
//!     daemon uses to aggregate per-user sweep results.
//! - [`daemon`]: the public unit-struct [`CleanupDaemon`]
//!   and its 2 orchestration methods — `spawn` (the
//!   background `JoinHandle` producer) and `run_cycle`
//!   (one full cleanup pass).
//! - [`sessions`]: the 2 session-cleanup methods —
//!   public `cleanup_expired_sessions` (sweeps every
//!   user namespace) and private
//!   `cleanup_expired_sessions_for_user` (the per-user
//!   terminal-state + age filter).
//! - [`temp_files`]: the 2 temp-file methods — private
//!   `cleanup_temp_files` (TTL filter on `.agents/tmp/`)
//!   and private `walk_and_delete` (recursive walker
//!   with empty-dir pruning).
//! - [`checkpoints`]: the 3 checkpoint methods — private
//!   `rotate_checkpoints` (per-session cap enforcement),
//!   private `rotate_checkpoints_for_user`, and private
//!   `cleanup_orphaned_checkpoints` (removes
//!   `checkpoints/` for sessions whose metadata is
//!   gone).
//! - [`util`]: the 2 private file-size helpers —
//!   `dir_size` and `file_size` — used by sessions and
//!   checkpoints for `bytes_reclaimed` accounting.
//!
//! Unit tests live in [`tests`].

mod checkpoints;
mod daemon;
mod sessions;
mod temp_files;
mod types;
mod util;

#[cfg(test)]
mod tests;

pub use daemon::CleanupDaemon;
pub use sessions::cleanup_expired_sessions;
pub use types::{CleanupConfig, CleanupMetrics};
