//! [`walk_ancestors`] — the filesystem walk that produces a
//! `Vec<DiscoveredFile>` in **farthest-to-closest** order.
//!
//! Handles symlink-cycle detection via canonical paths (so a symlink
//! loop doesn't cause infinite traversal) and silently skips files
//! that fail to read (so a permission error on one file doesn't kill
//! the whole section).

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use super::config::DiscoveredFile;

/// Walk `workspace_dir`'s ancestors (deepest first), collecting every
/// file whose name matches one of `filenames`. Returns the list in
/// **farthest-to-closest** order (filesystem root first, `workspace_dir`
/// last).
pub(super) fn walk_ancestors(
    workspace_dir: &Path,
    filenames: &[String],
) -> Vec<DiscoveredFile> {
    let mut out: Vec<DiscoveredFile> = Vec::new();
    let mut visited_canonicals: HashSet<PathBuf> = HashSet::new();

    // `Path::ancestors()` yields `workspace_dir` first, then walks up
    // to the root. We want farthest-first, so collect then reverse.
    let chain: Vec<PathBuf> =
        workspace_dir.ancestors().map(Path::to_path_buf).collect();

    for ancestor in chain.into_iter().rev() {
        for filename in filenames {
            let candidate = ancestor.join(filename);
            if !candidate.is_file() {
                continue;
            }
            let canonical = match std::fs::canonicalize(&candidate) {
                Ok(c) => c,
                Err(_) => {
                    // Canonicalize failure (e.g. file removed between
                    // is_file() and canonicalize()). Skip silently.
                    continue;
                }
            };
            if !visited_canonicals.insert(canonical.clone()) {
                // Already seen this canonical path → symlink cycle.
                tracing::debug!(
                    path = %candidate.display(),
                    canonical = %canonical.display(),
                    "agents_md: skipping duplicate canonical path (symlink cycle)"
                );
                continue;
            }
            match std::fs::read_to_string(&candidate) {
                Ok(content) => {
                    tracing::trace!(
                        path = %candidate.display(),
                        chars = content.chars().count(),
                        "agents_md: read file"
                    );
                    out.push(DiscoveredFile {
                        path: candidate,
                        content,
                    });
                }
                Err(e) => {
                    tracing::warn!(
                        path = %candidate.display(),
                        error = %e,
                        "agents_md: failed to read file, skipping"
                    );
                }
            }
        }
    }

    out
}
