//! The traversal defenses: agent-name whitelist, symlink
//! refusal, and directory walks that never leave the anchor.
//!
//! See the `memory::file` module docs (`# Defenses`) for the
//! posture these enforce.

use std::{fs, io::ErrorKind, path::Path};

use super::{MAX_AGENT_NAME_LEN, error::FileMemoryError};

/// Reject agent names outside the `[A-Za-z0-9_-]` whitelist. This is
/// the traversal defense: with no separators and no dots, the name
/// can never escape its scope directory.
pub(super) fn validate_agent(agent: &str) -> Result<(), FileMemoryError> {
    let safe = !agent.is_empty()
        && agent.len() <= MAX_AGENT_NAME_LEN
        && agent
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if safe {
        return Ok(());
    }
    Err(FileMemoryError::InvalidAgentName(agent.to_string()))
}

/// Refuse a path that is a symlink. A missing path is fine — it is
/// created (or read as empty) further down.
pub(super) fn refuse_symlink(path: &Path) -> Result<(), FileMemoryError> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            Err(FileMemoryError::Symlink(path.to_path_buf()))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error(path, error)),
    }
}

/// Walk `relative` below `base`, refusing symlinks at every level.
/// With `create`, missing levels are created — never through a
/// symlink, which is what makes the walk a defense rather than a
/// convenience.
pub(super) fn check_dir_below(
    base: &Path,
    relative: &Path,
    create: bool,
) -> Result<(), FileMemoryError> {
    let mut current = base.to_path_buf();
    for component in relative.components() {
        current.push(component);
        refuse_symlink(&current)?;
        if !create {
            continue;
        }
        match fs::create_dir(&current) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
            Err(error) => return Err(io_error(&current, error)),
        }
    }
    Ok(())
}

pub(super) fn io_error(path: &Path, error: std::io::Error) -> FileMemoryError {
    FileMemoryError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    }
}
