use std::path::{Path, PathBuf};

/// Error type for server-side workspace operations.
#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("workspace initialization failed: {message}")]
    WorkspaceInit { message: String },
}

/// Information about a validated or initialized workspace.
#[derive(Debug)]
pub struct WorkspaceInfo {
    pub root: PathBuf,
    pub is_new: bool,
}

/// Ensure the workspace exists at the given path.
/// If `.agents/` doesn't exist, create it. Returns workspace info.
pub fn ensure_workspace(
    workspace_root: &Path,
) -> Result<WorkspaceInfo, WorkspaceError> {
    let is_new = !workspace_root.join(".agents").exists();
    synthia_core::ensure_workspace_dirs(workspace_root)?;
    Ok(WorkspaceInfo {
        root: workspace_root.to_path_buf(),
        is_new,
    })
}

/// Initialize a new workspace at the given path.
/// Creates all required `.agents/` subdirectories unconditionally.
pub fn init_workspace(
    workspace_root: &Path,
) -> Result<WorkspaceInfo, WorkspaceError> {
    synthia_core::ensure_workspace_dirs(workspace_root)?;
    Ok(WorkspaceInfo {
        root: workspace_root.to_path_buf(),
        is_new: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let info = init_workspace(dir.path()).unwrap();
        assert!(info.is_new);
        assert!(dir.path().join(".agents/memory").exists());
        assert!(dir.path().join(".agents/skills").exists());
        assert!(dir.path().join(".agents/sessions").exists());
    }

    #[test]
    fn test_ensure_workspace_existing() {
        let dir = tempfile::tempdir().unwrap();
        init_workspace(dir.path()).unwrap();
        let info = ensure_workspace(dir.path()).unwrap();
        assert!(!info.is_new);
    }

    #[test]
    fn test_ensure_workspace_new() {
        let dir = tempfile::tempdir().unwrap();
        let info = ensure_workspace(dir.path()).unwrap();
        assert!(info.is_new);
    }

    #[test]
    fn test_workspace_creates_agents_directory() {
        let dir = tempfile::tempdir().unwrap();
        init_workspace(dir.path()).unwrap();
        assert!(dir.path().join(".agents").is_dir());
    }

    #[test]
    fn test_ensure_workspace_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        ensure_workspace(dir.path()).unwrap();
        ensure_workspace(dir.path()).unwrap();
        assert!(dir.path().join(".agents").exists());
    }
}
