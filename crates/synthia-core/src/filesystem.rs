use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::SystemTime,
};

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::Error;

pub struct FileMetadata {
    pub is_file: bool,
    pub is_dir: bool,
    pub len: u64,
    pub modified: Option<SystemTime>,
}

#[async_trait]
pub trait FileSystem: Send + Sync {
    async fn read_to_string(&self, path: &Path) -> std::io::Result<String>;
    async fn write(&self, path: &Path, contents: &str) -> std::io::Result<()>;
    async fn exists(&self, path: &Path) -> bool;
    async fn remove_file(&self, path: &Path) -> std::io::Result<()>;
    async fn read_dir(&self, path: &Path) -> std::io::Result<Vec<String>>;
    async fn create_dir_all(&self, path: &Path) -> std::io::Result<()>;
    async fn metadata(&self, path: &Path) -> std::io::Result<FileMetadata>;
}

pub struct OsFileSystem;

#[async_trait]
impl FileSystem for OsFileSystem {
    async fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
        tokio::fs::read_to_string(path).await
    }

    async fn write(&self, path: &Path, contents: &str) -> std::io::Result<()> {
        tokio::fs::write(path, contents).await
    }

    async fn exists(&self, path: &Path) -> bool {
        tokio::fs::metadata(path).await.is_ok()
    }

    async fn remove_file(&self, path: &Path) -> std::io::Result<()> {
        tokio::fs::remove_file(path).await
    }

    async fn read_dir(&self, path: &Path) -> std::io::Result<Vec<String>> {
        let mut entries = tokio::fs::read_dir(path).await?;
        let mut result = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            if let Some(name) = entry.file_name().to_str() {
                result.push(name.to_string());
            }
        }
        Ok(result)
    }

    async fn create_dir_all(&self, path: &Path) -> std::io::Result<()> {
        tokio::fs::create_dir_all(path).await
    }

    async fn metadata(&self, path: &Path) -> std::io::Result<FileMetadata> {
        let meta = tokio::fs::metadata(path).await?;
        Ok(FileMetadata {
            is_file: meta.is_file(),
            is_dir: meta.is_dir(),
            len: meta.len(),
            modified: meta.modified().ok(),
        })
    }
}

pub struct InMemoryFileSystem {
    files: RwLock<HashMap<String, Vec<u8>>>,
}

impl InMemoryFileSystem {
    pub fn new() -> Self {
        Self {
            files: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryFileSystem {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FileSystem for InMemoryFileSystem {
    async fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
        let files = self.files.read().await;
        let key = path.to_string_lossy().to_string();
        match files.get(&key) {
            Some(data) => String::from_utf8(data.clone()).map_err(|e| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, e)
            }),
            None => Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "file not found",
            )),
        }
    }

    async fn write(&self, path: &Path, contents: &str) -> std::io::Result<()> {
        let mut files = self.files.write().await;
        let key = path.to_string_lossy().to_string();
        files.insert(key, contents.as_bytes().to_vec());
        Ok(())
    }

    async fn exists(&self, path: &Path) -> bool {
        let files = self.files.read().await;
        let key = path.to_string_lossy().to_string();
        files.contains_key(&key)
    }

    async fn remove_file(&self, path: &Path) -> std::io::Result<()> {
        let mut files = self.files.write().await;
        let key = path.to_string_lossy().to_string();
        files.remove(&key).ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "file not found")
        })?;
        Ok(())
    }

    async fn read_dir(&self, path: &Path) -> std::io::Result<Vec<String>> {
        let files = self.files.read().await;
        let prefix = path.to_string_lossy().to_string();
        let prefix_with_sep = if prefix.ends_with('/') {
            prefix.clone()
        } else {
            format!("{}/", prefix)
        };
        let mut entries: Vec<String> = files
            .keys()
            .filter(|k| k.starts_with(&prefix_with_sep))
            .filter_map(|k| k.strip_prefix(&prefix_with_sep))
            .filter_map(|s| s.split('/').next())
            .map(|s| s.to_string())
            .collect();
        entries.sort();
        entries.dedup();
        Ok(entries)
    }

    async fn create_dir_all(&self, _path: &Path) -> std::io::Result<()> {
        Ok(())
    }

    async fn metadata(&self, path: &Path) -> std::io::Result<FileMetadata> {
        let files = self.files.read().await;
        let key = path.to_string_lossy().to_string();
        match files.get(&key) {
            Some(data) => Ok(FileMetadata {
                is_file: true,
                is_dir: false,
                len: data.len() as u64,
                modified: None,
            }),
            None => {
                let prefix = if key.ends_with('/') {
                    key.clone()
                } else {
                    format!("{}/", key)
                };
                let is_dir = files.keys().any(|k| k.starts_with(&prefix));
                if is_dir {
                    Ok(FileMetadata {
                        is_file: false,
                        is_dir: true,
                        len: 0,
                        modified: None,
                    })
                } else {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "file not found",
                    ))
                }
            }
        }
    }
}

// ── Path Safety Utilities ─────────────────────────────────────────────────

pub struct PathChecker {
    workspace_root: PathBuf,
}

impl PathChecker {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }

    pub fn check_path_safety(&self, path: &str) -> Result<PathBuf, Error> {
        let resolved = self.workspace_root.join(path);
        let canonical = resolved
            .canonicalize()
            .map_err(|_| Error::Forbidden(format!("Invalid path: {}", path)))?;
        let workspace_canonical =
            self.workspace_root.canonicalize().map_err(|_| {
                Error::Forbidden("Invalid workspace path".to_string())
            })?;
        if !canonical.starts_with(&workspace_canonical) {
            return Err(Error::Forbidden(
                "Path traversal not allowed".to_string(),
            ));
        }
        Ok(canonical)
    }
}

/// Validates that a path is within the workspace root (sandbox check).
/// Returns an error if the path escapes the workspace.
pub fn validate_path(path: &Path, workspace_root: &Path) -> Result<(), Error> {
    let resolved_root = workspace_root.canonicalize().map_err(|e| {
        Error::Forbidden(format!("cannot resolve workspace root: {e}"))
    })?;

    let resolved = path.canonicalize().ok().unwrap_or_else(|| {
        let full = if path.is_absolute() {
            path.to_path_buf()
        } else {
            workspace_root.join(path)
        };
        lexical_normalize(&full)
    });

    if !resolved.starts_with(&resolved_root) {
        return Err(Error::Forbidden(format!(
            "path {} is outside workspace {}",
            resolved.display(),
            resolved_root.display()
        )));
    }

    Ok(())
}

/// Lexically normalizes a path by resolving `.` and `..` components without touching the filesystem.
fn lexical_normalize(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for comp in path.components() {
        match comp {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                components.pop();
            }
            _ => components.push(comp.as_os_str().to_owned()),
        }
    }
    let mut result = PathBuf::new();
    for c in components {
        result.push(c);
    }
    result
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_path_rejects_escape() {
        let temp = tempfile::tempdir().unwrap();
        let result = validate_path(Path::new("/etc/passwd"), temp.path());
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Forbidden(_)));
    }

    #[test]
    fn test_validate_path_rejects_traversal() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path();
        let result = validate_path(Path::new("../../../etc/passwd"), workspace);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_path_accepts_within_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path();
        std::fs::write(workspace.join("hello.txt"), "test").unwrap();
        let result = validate_path(&workspace.join("hello.txt"), workspace);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_path_sibling_dir_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path();
        let sibling = temp.path().parent().unwrap().join("sibling");
        std::fs::create_dir_all(&sibling).unwrap();
        std::fs::write(sibling.join("evil.txt"), "").unwrap();
        let result = validate_path(&sibling.join("evil.txt"), workspace);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Forbidden(_)));
    }

    #[test]
    fn test_path_traversal_blocked() {
        let temp = tempfile::tempdir().unwrap();
        let checker = PathChecker::new(temp.path().to_path_buf());
        let result = checker.check_path_safety("../etc/passwd");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Forbidden(_)));
    }

    #[test]
    fn test_valid_path_allowed() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("hello.txt"), "test").unwrap();
        let checker = PathChecker::new(temp.path().to_path_buf());
        let result = checker.check_path_safety("hello.txt");
        assert!(result.is_ok());
        assert!(result.unwrap().starts_with(temp.path()));
    }

    #[test]
    fn test_symlink_escaping_blocked() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.txt"), "secret").unwrap();

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, workspace.join("escape"))
                .unwrap();
        }

        let checker = PathChecker::new(workspace.clone());
        let result = checker.check_path_safety("escape/secret.txt");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Forbidden(_)));
    }
}
