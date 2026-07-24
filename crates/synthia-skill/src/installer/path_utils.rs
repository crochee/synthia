//! Path-level security helpers for archive
//! extraction.
//!
//! Two free functions live here, both pure (no
//! filesystem access):
//!
//! - [`strip_top_level_prefix`]: when a `.skill`
//!   archive is built from a directory
//!   `my-skill/`, every entry's path is
//!   `my-skill/...`. The installer wants the
//!   skill's contents laid out flat under
//!   `skills_dir/<name>/`, so this helper
//!   strips the leading directory component.
//!   It only strips a single level, and only if
//!   the first component is a
//!   `Component::Normal(_)` — `..` and absolute
//!   paths are passed through unchanged and
//!   later rejected by the security checks.
//! - [`has_path_traversal`]: returns `true` if
//!   any component of the path is a
//!   `Component::ParentDir`. Used to reject
//!   `../../etc/passwd`-style entries in the
//!   archive.
//!
//! Kept separate from [`super::installer`] so the
//! path-security rules can be unit-tested in
//! isolation (see [`super::tests`]) and from
//! [`super::package`] which is archive-content
//! oriented.

use std::path::{Path, PathBuf};

/// Strip the top-level directory prefix from a
/// path. E.g., `"my-skill/SKILL.md"` →
/// `"SKILL.md"`.
pub(super) fn strip_top_level_prefix(path: &Path) -> PathBuf {
    let mut components = path.components();
    // Skip the first component if it's a normal directory component
    if let Some(first) = components.next()
        && matches!(first, std::path::Component::Normal(_))
    {
        return components.collect();
    }
    path.to_path_buf()
}

/// Check if a path contains traversal sequences
/// (i.e., any `..` component).
pub(super) fn has_path_traversal(path: &Path) -> bool {
    for component in path.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return true;
        }
    }
    false
}
