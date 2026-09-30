//! Errors the file-backed long-term tier can surface.

use std::path::PathBuf;

/// Errors the file-backed long-term tier can surface.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FileMemoryError {
    /// The agent name is empty or carries characters outside the
    /// `[A-Za-z0-9_-]` whitelist (traversal can never reach the
    /// filesystem).
    #[error("unsafe memory agent name: {0:?}")]
    InvalidAgentName(String),
    /// A memory path is a symlink; the tier refuses to follow it.
    #[error("refusing symlinked memory path: {0}")]
    Symlink(PathBuf),
    /// The configuration lists no scopes.
    #[error("memory configuration lists no scopes")]
    NoScopes,
    /// The `user` scope needs a home directory and neither
    /// `user_base` nor `$HOME` is available.
    #[error("the user memory scope needs `user_base` or $HOME")]
    MissingHome,
    /// The tier is read-only; the write was refused.
    #[error("memory tier is read-only: {0}")]
    ReadOnly(PathBuf),
    /// Filesystem failure.
    #[error("memory io failure at {path}: {message}")]
    Io {
        /// Offending path.
        path: PathBuf,
        /// Underlying error message.
        message: String,
    },
}
