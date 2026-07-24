use std::path::{Path, PathBuf};

pub fn resolve_path(workspace_root: &Path, path: &str) -> PathBuf {
    let p = PathBuf::from(path);
    if p.is_absolute() {
        p
    } else {
        workspace_root.join(p)
    }
}

/// Canonicalize a path even if it (or some ancestor) does not exist on disk.
/// This walks the path component-by-component, canonicalizing each existing
/// prefix. For the non-existing tail, components are appended literally.
///
/// Returns the canonical path if it can be determined, otherwise the
/// lexical-resolved path (which preserves `..` components for the
/// `starts_with` check below to catch).
fn safe_canonicalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        result.push(component.as_os_str());
        // Try to canonicalize what we have so far; if it exists, replace
        // `result` with its canonical form (which resolves any `..`).
        if let Ok(canon) = result.canonicalize() {
            result = canon;
        }
    }
    result
}

pub fn check_path_safety(workspace_root: &Path, path: &str) -> Option<String> {
    let resolved = resolve_path(workspace_root, path);
    let canonical_root = workspace_root
        .canonicalize()
        .unwrap_or_else(|_| workspace_root.to_path_buf());
    let canonical_path = safe_canonicalize(&resolved);
    if !canonical_path.starts_with(&canonical_root) {
        return Some(format!("Path {} is outside workspace", path));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_relative() {
        let root = Path::new("/workspace");
        let resolved = resolve_path(root, "src/lib.rs");
        assert_eq!(resolved, PathBuf::from("/workspace/src/lib.rs"));
    }

    #[test]
    fn test_resolve_absolute() {
        let root = Path::new("/workspace");
        let resolved = resolve_path(root, "/other/file.txt");
        assert_eq!(resolved, PathBuf::from("/other/file.txt"));
    }

    #[test]
    fn test_check_path_safety_inside() {
        let dir = tempfile::tempdir().unwrap();
        assert!(check_path_safety(dir.path(), "file.txt").is_none());
    }

    #[test]
    fn test_check_path_safety_traversal_blocked() {
        let dir = tempfile::tempdir().unwrap();
        let result = check_path_safety(dir.path(), "../../../etc/passwd");
        assert!(result.is_some());
        assert!(result.unwrap().contains("outside workspace"));
    }
}
