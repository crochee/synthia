use std::path::{Path, PathBuf};

pub fn resolve_workspace_path(workspace_root: &Path, module: &str) -> PathBuf {
    workspace_root.join(".agents").join(module)
}

pub fn ensure_workspace_dirs(workspace_root: &Path) -> std::io::Result<()> {
    let dirs = [
        "memory",
        "skills",
        "sessions",
        "commands",
        "checkpoints",
        "mailbox",
    ];
    for dir in &dirs {
        let path = resolve_workspace_path(workspace_root, dir);
        std::fs::create_dir_all(&path)?;
    }
    Ok(())
}

pub fn sanitize_path_segment(segment: &str) -> String {
    segment
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_workspace_path() {
        let root = Path::new("/tmp/workspace");
        let path = resolve_workspace_path(root, "memory");
        assert_eq!(path, PathBuf::from("/tmp/workspace/.agents/memory"));
    }

    #[test]
    fn test_resolve_workspace_path_sessions() {
        let root = Path::new("/tmp/workspace");
        let path = resolve_workspace_path(root, "sessions");
        assert_eq!(path, PathBuf::from("/tmp/workspace/.agents/sessions"));
    }

    #[test]
    fn test_ensure_workspace_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let result = ensure_workspace_dirs(dir.path());
        assert!(result.is_ok());

        assert!(dir.path().join(".agents/memory").exists());
        assert!(dir.path().join(".agents/skills").exists());
        assert!(dir.path().join(".agents/sessions").exists());
        assert!(dir.path().join(".agents/commands").exists());
        assert!(dir.path().join(".agents/checkpoints").exists());
        assert!(dir.path().join(".agents/mailbox").exists());
    }

    #[test]
    fn test_sanitize_path_segment() {
        assert_eq!(sanitize_path_segment("hello"), "hello");
        assert_eq!(sanitize_path_segment("hello-world"), "hello-world");
        assert_eq!(sanitize_path_segment("hello world!"), "helloworld");
        assert_eq!(sanitize_path_segment("a_b"), "a_b");
    }
}
