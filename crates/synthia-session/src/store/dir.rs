//! Directory path helpers and namespace-wide listing.
//!
//! These helpers operate on `&Path` so they can be unit-tested in
//! isolation and reused by both the on-disk store and privileged
//! callers that iterate the raw tree.

use std::{
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

use super::types::SessionMetadata;
use crate::error::StoreError;

/// Returns `{root}/{user_id}/{session_id}`.
pub(crate) fn session_path(
    root: &Path,
    user_id: &str,
    session_id: &str,
) -> PathBuf {
    root.join(user_id).join(session_id)
}

/// Returns `{root}/{user_id}`.
pub(crate) fn user_path(root: &Path, user_id: &str) -> PathBuf {
    root.join(user_id)
}

/// Ensure the session directory exists. Sets 0o700 permissions on
/// Unix so that only the owning user can read or modify the
/// session's contents; the user_id namespace prevents other users
/// from reaching this directory at all.
pub(crate) fn ensure_session_dir(dir: &Path) -> Result<PathBuf> {
    fs::create_dir_all(dir)
        .with_context(|| format!("Failed to create session dir: {:?}", dir))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
            .with_context(|| {
                format!("Failed to set 0o700 on session dir: {:?}", dir)
            })?;
    }
    Ok(dir.to_path_buf())
}

/// Count non-empty lines in `messages.jsonl` under `dir`. Returns 0
/// when the file does not exist yet.
pub(crate) fn count_messages_in(dir: &Path) -> Result<usize> {
    let path = dir.join("messages.jsonl");
    if !path.exists() {
        return Ok(0);
    }
    let file = fs::File::open(&path)?;
    let reader = BufReader::new(file);
    let count = reader
        .lines()
        .map_while(Result::ok)
        .filter(|l| !l.trim().is_empty())
        .count();
    Ok(count)
}

/// List all user_ids that currently have a session directory
/// under `sessions_root`. Returns them in arbitrary (filesystem)
/// order; deterministic ordering is not part of the contract.
///
/// The returned set includes reserved namespaces such as
/// [`super::types::SERVER_DEFAULT_USER_ID`] — they are real user
/// directories on disk and the cleanup daemon must sweep them.
/// Callers that need to skip the default namespace can filter
/// against `SERVER_DEFAULT_USER_ID` themselves.
///
/// Files (and symlinks) that are not directories are filtered
/// out. `.` / `..` are never returned. A non-existent
/// `sessions_root` returns an empty vec rather than an error
/// (the directory may not exist yet on a fresh install).
pub(crate) fn list_user_ids(root: &Path) -> Result<Vec<String>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut ids = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if !file_type.is_dir() {
            continue;
        }
        // `OsString::to_str` returns a borrow tied to the
        // `OsString` temporary; convert to owned `String` so the
        // borrow doesn't outlive this iteration.
        let name = match entry.file_name().into_string() {
            Ok(s) => s,
            Err(_) => continue, // Non-UTF-8 name: skip silently.
        };
        if name.is_empty() || name == "." || name == ".." {
            continue;
        }
        ids.push(name);
    }
    Ok(ids)
}

/// List all session ids under the given user's namespace.
///
/// Does NOT list other users' sessions: each user has their own
/// directory and the store does not cross user_id boundaries.
pub(crate) fn list_session_ids(
    root: &Path,
    user_id: &str,
) -> Result<Vec<String>> {
    let user_dir = user_path(root, user_id);
    if !user_dir.exists() {
        return Ok(Vec::new());
    }

    let mut ids = Vec::new();
    for entry in fs::read_dir(&user_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir()
            && let Some(name) = entry.file_name().to_str()
        {
            ids.push(name.to_string());
        }
    }
    Ok(ids)
}

/// List sessions visible to `caller_user_id`, returning their
/// metadata.
///
/// Cross-user access is structurally impossible at the directory
/// level: the store only walks the caller's user directory. If a
/// metadata file is found whose `owner_user_id` does NOT match the
/// caller's user_id (a corrupted or forged metadata file), the
/// store refuses the entire listing by returning
/// [`StoreError::CrossUserAccess`].
pub(crate) fn list_sessions_with_metadata(
    root: &Path,
    caller_user_id: &str,
) -> Result<Vec<SessionMetadata>> {
    if caller_user_id.is_empty() {
        return Err(StoreError::EmptyUserId {
            session_id: "<list>".to_string(),
        }
        .into());
    }
    let ids = list_session_ids(root, caller_user_id)?;
    let mut result = Vec::with_capacity(ids.len());
    for id in ids {
        let meta = super::metadata::load_from(root, caller_user_id, &id)?;
        // `load_from` already enforces owner_user_id == caller_user_id
        // and returns CrossUserAccess on mismatch, so the duplicate
        // check below is defense-in-depth.
        if meta.owner_user_id != caller_user_id {
            return Err(StoreError::CrossUserAccess {
                caller: caller_user_id.to_string(),
                owner: meta.owner_user_id.clone(),
            }
            .into());
        }
        result.push(meta);
    }
    Ok(result)
}
