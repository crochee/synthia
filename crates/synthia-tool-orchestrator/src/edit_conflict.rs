//! Edit conflict detection for file-mutating tools.
//!
//! Detects when a user and the agent edit the same file concurrently,
//! preventing silent data loss. Uses content hashing to detect changes
//! since the agent read the file.

use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hasher},
    path::PathBuf,
    sync::Arc,
    time::UNIX_EPOCH,
};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// Information about a recorded file read.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSnapshot {
    pub path: PathBuf,
    pub mtime_ns: u64,
    pub content_hash: u64,
}

/// Conflict information returned when a conflict is detected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictInfo {
    pub path: PathBuf,
    pub agent_snapshot: FileSnapshot,
    pub current_mtime_ns: u64,
    pub current_hash: u64,
}

/// Computes a fast non-cryptographic hash of file content.
/// Suitable for change detection, not security.
fn hash_content(content: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    h.write(content);
    h.finish()
}

/// Gets the modification time of a file in nanoseconds since epoch.
fn get_mtime_ns(path: &PathBuf) -> Option<u64> {
    std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .map(|t| {
            t.duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0)
        })
}

/// Records a file read for conflict detection.
pub async fn record_read(
    path: &PathBuf,
    content: &[u8],
    snapshot_store: &Arc<RwLock<HashMap<PathBuf, FileSnapshot>>>,
) {
    let content_hash = hash_content(content);
    let mtime_ns = get_mtime_ns(path).unwrap_or(0);
    let snapshot = FileSnapshot {
        path: path.clone(),
        mtime_ns,
        content_hash,
    };
    snapshot_store.write().await.insert(path.clone(), snapshot);
}

/// Checks if the file has been modified since it was last recorded.
/// Returns `None` if no conflict (file unchanged or not tracked).
/// Returns `Some(ConflictInfo)` if conflict detected.
pub async fn check_conflict(
    path: &PathBuf,
    snapshot_store: &Arc<RwLock<HashMap<PathBuf, FileSnapshot>>>,
) -> Option<ConflictInfo> {
    let snapshot = snapshot_store.read().await.get(path).cloned()?;
    let current_mtime = get_mtime_ns(path)?;
    if current_mtime > snapshot.mtime_ns
        && let Ok(current_content) = std::fs::read(path)
    {
        let current_hash = hash_content(&current_content);
        if current_hash != snapshot.content_hash {
            return Some(ConflictInfo {
                path: path.clone(),
                agent_snapshot: snapshot,
                current_mtime_ns: current_mtime,
                current_hash,
            });
        }
    }
    None
}

/// Clears all recorded snapshots.
pub async fn clear_all(
    snapshot_store: &Arc<RwLock<HashMap<PathBuf, FileSnapshot>>>,
) {
    snapshot_store.write().await.clear();
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[tokio::test]
    async fn test_no_conflict_when_file_unchanged() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test.txt");
        std::fs::write(&path, "hello").unwrap();

        let store: Arc<RwLock<_>> = Arc::new(RwLock::new(HashMap::new()));
        record_read(&path, b"hello", &store).await;
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let conflict = check_conflict(&path, &store).await;
        assert!(conflict.is_none());
    }

    #[tokio::test]
    async fn test_conflict_when_file_modified() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test.txt");
        std::fs::write(&path, "hello").unwrap();

        let store: Arc<RwLock<_>> = Arc::new(RwLock::new(HashMap::new()));
        record_read(&path, b"hello", &store).await;
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        std::fs::write(&path, "world").unwrap();

        let conflict = check_conflict(&path, &store).await;
        assert!(conflict.is_some());
        assert_eq!(
            conflict.unwrap().agent_snapshot.content_hash,
            hash_content(b"hello")
        );
    }

    #[tokio::test]
    async fn test_no_conflict_for_untracked_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test.txt");
        std::fs::write(&path, "hello").unwrap();

        let store: Arc<RwLock<_>> = Arc::new(RwLock::new(HashMap::new()));
        let conflict = check_conflict(&path, &store).await;
        assert!(conflict.is_none());
    }
}
