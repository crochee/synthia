//! Workspace path confinement — the file-touching half of "what a
//! call is allowed to touch".
//!
//! A file tool (`read`, `write`, or a consumer's own `edit` /
//! `grep`) resolves a user- or model-supplied path against the
//! run's workspace root and refuses anything that escapes it.
//! The rule lives here — not inside each tool — so every
//! file-touching tool enforces the *same* boundary:
//!
//! - an absolute path is taken as-is, a relative path is joined
//!   onto the workspace root;
//! - the path is canonicalized component-by-component (so a
//!   non-existent tail does not hide a `..` that points outside);
//! - the canonical result must stay under the canonical root.
//!
//! This is the path half of "what a call is allowed to touch".
//! The process half — OS-level execution policy, `ExecutionPolicy`
//! and `SandboxBackend` — lives in `synthia-tool-shell`, the plugin
//! that actually spawns processes, so this crate carries no process
//! policy at all.

use std::path::{Path, PathBuf};

/// Resolve `path` against `workspace_root`.
///
/// Absolute paths pass through unchanged; relative paths are
/// joined onto the root. No canonicalization happens here —
/// pair with [`check_path_safety`] before touching the file.
#[must_use]
pub fn resolve_path(workspace_root: &Path, path: &str) -> PathBuf {
    let p = PathBuf::from(path);
    if p.is_absolute() {
        p
    } else {
        workspace_root.join(p)
    }
}

/// Canonicalize a path even if it (or some ancestor) does not exist on disk.
///
/// Walks the path component-by-component, canonicalizing each existing
/// prefix. `..` is resolved **lexically** when its prefix cannot be
/// canonicalized: without that, a path like `<workspace>/missing/../../etc`
/// would keep its `..` segments, still *look* like it starts with the
/// workspace, and reach the escape as soon as `missing/` exists.
fn safe_canonicalize(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                if !result.pop() {
                    result.push(component.as_os_str());
                }
            }
            _ => {
                result.push(component.as_os_str());
                // Try to canonicalize what we have so far; if it exists,
                // replace `result` with its canonical form (which resolves
                // symlinks and any remaining `..`).
                if let Ok(canon) = result.canonicalize() {
                    result = canon;
                }
            }
        }
    }
    result
}

/// Return the refusal message when `path` escapes `workspace_root`,
/// `None` when it stays inside.
///
/// ```rust
/// use std::path::Path;
///
/// use synthia_tool::workspace::check_path_safety;
///
/// let root = Path::new("/tmp/synthia-workspace");
/// assert!(check_path_safety(root, "../../etc/passwd").is_some());
/// assert_eq!(check_path_safety(root, "src/main.rs"), None);
/// ```
#[must_use]
pub fn check_path_safety(workspace_root: &Path, path: &str) -> Option<String> {
    let resolved = resolve_path(workspace_root, path);
    let canonical_root = workspace_root
        .canonicalize()
        .unwrap_or_else(|_| workspace_root.to_path_buf());
    let canonical_path = safe_canonicalize(&resolved);
    if !canonical_path.starts_with(&canonical_root) {
        return Some(format!("Path {path} is outside workspace"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths_join_the_root() {
        let resolved = resolve_path(Path::new("/ws"), "src/a.rs");
        assert_eq!(resolved, Path::new("/ws/src/a.rs"));
    }

    #[test]
    fn absolute_paths_pass_through() {
        let resolved = resolve_path(Path::new("/ws"), "/etc/hostname");
        assert_eq!(resolved, Path::new("/etc/hostname"));
    }

    #[test]
    fn escape_via_dotdot_is_refused() {
        let dir = tempfile::tempdir().expect("tempdir");
        let msg = check_path_safety(dir.path(), "../outside.txt");
        assert!(msg.is_some());
        assert!(msg.unwrap().contains("outside workspace"));
    }

    #[test]
    fn escape_via_missing_symlink_tail_is_refused() {
        let dir = tempfile::tempdir().expect("tempdir");
        // The tail does not exist, but the `..` still escapes.
        let msg = check_path_safety(dir.path(), "a/../../elsewhere");
        assert!(msg.is_some());
    }

    #[test]
    fn inside_paths_are_allowed() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(check_path_safety(dir.path(), "a/b/c.txt").is_none());
        assert!(check_path_safety(dir.path(), "./nested").is_none());
    }
}
