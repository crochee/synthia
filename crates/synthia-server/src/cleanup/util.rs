//! File-size helpers used by [`super::sessions`] and
//! [`super::checkpoints`] for `bytes_reclaimed`
//! accounting. Both are private — callers go through the
//! daemon's per-concern entry points.

use std::{fs, path::Path};

/// Calculate the total size of a directory in bytes.
pub(super) fn dir_size(dir: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                total += dir_size(&path);
            } else if let Ok(metadata) = entry.metadata() {
                total += metadata.len();
            }
        }
    }
    total
}

/// Get the size of a single file in bytes.
///
/// Returns `0` on `fs::metadata` failure (e.g. file was
/// concurrently removed between the size lookup and the
/// actual delete). The caller is the one that decides
/// whether to swallow the IO error.
pub(super) fn file_size(path: &Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}
