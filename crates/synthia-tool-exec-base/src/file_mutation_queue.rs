//! Per-filepath serialization for file-mutating tools.
//!
//! `write_file` / `apply_patch` / `edit_file` acquire a per-filepath
//! mutex before performing any write operation. The mutex map is keyed
//! by the canonicalized realpath (resolving symlinks). Uses
//! `tokio::sync::Semaphore` (safe across `.await` points).
//!
//! Idle entries are cleaned up after the semaphore permit is released
//! to prevent unbounded memory growth across long-running sessions.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};

/// A guard that releases the per-filepath semaphore permit when dropped.
///
/// Also cleans up the map entry if no other task is waiting on the same
/// filepath, preventing unbounded memory growth.
pub struct FileMutationGuard {
    map: Arc<Mutex<HashMap<PathBuf, Arc<Semaphore>>>>,
    key: PathBuf,
    /// The owned permit. Dropping this releases the semaphore.
    /// Fields are dropped in declaration order, so `_permit` is dropped
    /// AFTER `Drop::drop` runs — meaning the permit is still alive while
    /// we check `Arc::strong_count` in `Drop::drop`.
    _permit: OwnedSemaphorePermit,
}

impl Drop for FileMutationGuard {
    fn drop(&mut self) {
        // Try to clean up the map entry if no waiter exists.
        // `try_lock` avoids blocking in `Drop` (which is not async-aware).
        if let Ok(mut map) = self.map.try_lock()
            && let Some(sem) = map.get(&self.key)
        {
            // `Arc::strong_count` counts all strong references:
            //  - map holds 1
            //  - our `_permit` holds 1 (internally)
            //  - each waiter (a task blocked in `acquire`) holds 1
            // So `strong_count <= 2` means no waiter is blocked →
            // safe to remove. If > 2, at least one waiter exists →
            // keep the entry so the waiter can acquire it.
            if Arc::strong_count(sem) <= 2 {
                map.remove(&self.key);
            }
        }
        // After `Drop::drop` returns, `_permit` is dropped, releasing
        // the semaphore. If a waiter exists, its `acquire_owned` future
        // resolves immediately.
    }
}

/// Per-filepath serialization queue for file mutations.
///
/// Cloning is cheap — the inner map is shared via `Arc<Mutex<...>>`.
/// All clones share the same queue state.
#[derive(Clone, Default)]
pub struct FileMutationQueue {
    map: Arc<Mutex<HashMap<PathBuf, Arc<Semaphore>>>>,
}

impl FileMutationQueue {
    /// Create a new empty queue.
    pub fn new() -> Self {
        Self::default()
    }

    /// Acquire the per-filepath lock for the given path.
    ///
    /// The path is canonicalized (resolving symlinks) before being used
    /// as the map key. If canonicalization fails (e.g., the file doesn't
    /// exist yet for a new write), the original path is used as-is.
    ///
    /// Returns a guard that releases the lock when dropped.
    pub async fn acquire(&self, path: impl AsRef<Path>) -> FileMutationGuard {
        let key = canonicalize_key(path.as_ref());
        let semaphore = {
            let mut map = self.map.lock().await;
            map.entry(key.clone())
                .or_insert_with(|| Arc::new(Semaphore::new(1)))
                .clone()
        };
        // `acquire_owned` consumes the `Arc<Semaphore>` clone and returns
        // an `OwnedSemaphorePermit` that holds an `Arc<Semaphore>` internally.
        // After this, strong_count = 2 (map + permit's internal Arc).
        let _permit = semaphore
            .acquire_owned()
            .await
            .expect("semaphore is never closed");
        FileMutationGuard {
            map: Arc::clone(&self.map),
            key,
            _permit,
        }
    }

    /// Returns the number of entries currently in the queue's internal map.
    /// Useful for testing cleanup behavior.
    pub async fn len(&self) -> usize {
        self.map.lock().await.len()
    }

    /// Returns `true` if the queue's internal map is empty.
    pub async fn is_empty(&self) -> bool {
        self.map.lock().await.is_empty()
    }
}

/// Canonicalize a path to use as the map key.
/// Resolves symlinks via `std::fs::canonicalize`.
/// Falls back to the original path if canonicalization fails (e.g., the
/// file doesn't exist yet for a new write).
fn canonicalize_key(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[tokio::test]
    async fn same_filepath_serializes() {
        // Two acquires on the same realpath: the second must block
        // until the first guard is dropped.
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("file.txt");
        std::fs::write(&file, "").unwrap();

        let queue = FileMutationQueue::new();

        let guard1 = queue.acquire(&file).await;
        // The second acquire should not complete while guard1 is held.
        let queue_clone = queue.clone();
        let file_clone = file.clone();
        let handle = tokio::spawn(async move {
            let _g = queue_clone.acquire(&file_clone).await;
            // If we reach here, the guard was acquired after guard1 dropped.
        });

        // Give the spawned task a chance to run — it should be blocked.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(!handle.is_finished(), "second acquire should be blocked");

        drop(guard1);
        // Now the spawned task should complete.
        tokio::time::timeout(std::time::Duration::from_secs(1), handle)
            .await
            .expect("second acquire should complete after first guard drops")
            .unwrap();

        // After both guards drop, the entry should be cleaned up.
        assert!(queue.is_empty().await);
    }

    #[tokio::test]
    async fn different_filepaths_run_parallel() {
        // Two acquires on different realpaths: both proceed immediately.
        let dir = tempfile::tempdir().unwrap();
        let file_a = dir.path().join("a.txt");
        let file_b = dir.path().join("b.txt");
        std::fs::write(&file_a, "").unwrap();
        std::fs::write(&file_b, "").unwrap();

        let queue = FileMutationQueue::new();

        let guard_a = queue.acquire(&file_a).await;
        // Acquire on a different file should succeed immediately.
        let guard_b = queue.acquire(&file_b).await;

        // Both held simultaneously → parallel.
        assert_eq!(queue.len().await, 2);

        drop(guard_a);
        drop(guard_b);
        assert!(queue.is_empty().await);
    }

    #[tokio::test]
    async fn symlink_shares_realpath_key() {
        // A symlink and its target share the same lock because the key
        // is the canonicalized realpath.
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real.txt");
        let link = dir.path().join("link.txt");
        std::fs::write(&real, "").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let queue = FileMutationQueue::new();

        // Acquire via the real path.
        let guard_real = queue.acquire(&real).await;

        // A second acquire via the symlink should block.
        let queue_clone = queue.clone();
        let link_clone = link.clone();
        let handle = tokio::spawn(async move {
            let _g = queue_clone.acquire(&link_clone).await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(
            !handle.is_finished(),
            "acquire via symlink should block while real path is locked"
        );

        drop(guard_real);
        tokio::time::timeout(std::time::Duration::from_secs(1), handle)
            .await
            .expect("symlink acquire should complete after real guard drops")
            .unwrap();

        assert!(queue.is_empty().await);
    }

    #[tokio::test]
    async fn entry_removed_after_single_write() {
        // After a single write completes (guard drops) with no waiters,
        // the entry is removed from the map.
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("single.txt");
        std::fs::write(&file, "").unwrap();

        let queue = FileMutationQueue::new();
        assert!(queue.is_empty().await);

        {
            let _guard = queue.acquire(&file).await;
            assert_eq!(queue.len().await, 1);
        }
        // Guard dropped → entry cleaned up.
        assert!(queue.is_empty().await);
    }

    #[tokio::test]
    async fn entry_retained_while_waiter_exists() {
        // While a waiter is blocked, the entry is retained (not removed)
        // when the current holder drops its guard.
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("contended.txt");
        std::fs::write(&file, "").unwrap();

        let queue = FileMutationQueue::new();
        let guard = queue.acquire(&file).await;

        // Spawn a waiter that blocks until the guard is released, then
        // holds the guard until we tell it to release.
        let notify_acquired = Arc::new(tokio::sync::Notify::new());
        let notify_release = Arc::new(tokio::sync::Notify::new());
        let queue_clone = queue.clone();
        let file_clone = file.clone();
        let acquired_clone = notify_acquired.clone();
        let release_clone = notify_release.clone();
        let handle = tokio::spawn(async move {
            let _g = queue_clone.acquire(&file_clone).await;
            // Signal that we've acquired the lock.
            acquired_clone.notify_one();
            // Wait to be told to release.
            release_clone.notified().await;
            // _g dropped here when the task ends.
        });

        // Let the waiter reach the `acquire_owned` call (it's blocked).
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Drop the holder. The waiter should proceed. The entry must
        // NOT be removed by the holder's Drop because a waiter exists.
        drop(guard);

        // Wait for the waiter to signal it acquired the lock.
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            notify_acquired.notified(),
        )
        .await
        .expect("waiter should acquire the lock after holder drops");

        // The entry should still exist because the waiter now holds it.
        assert_eq!(
            queue.len().await,
            1,
            "entry should be retained while waiter holds the lock"
        );

        // Tell the waiter to release.
        notify_release.notify_one();

        // Wait for the waiter to complete and drop its guard.
        tokio::time::timeout(std::time::Duration::from_secs(1), handle)
            .await
            .expect("waiter should complete")
            .unwrap();

        // Now the entry should be cleaned up.
        assert!(queue.is_empty().await);
    }

    #[tokio::test]
    async fn clone_shares_state() {
        // Clones of the queue share the same underlying map.
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("shared.txt");
        std::fs::write(&file, "").unwrap();

        let queue1 = FileMutationQueue::new();
        let queue2 = queue1.clone();

        let guard = queue1.acquire(&file).await;
        // queue2 should see the entry created by queue1.
        assert_eq!(queue2.len().await, 1);
        drop(guard);
        assert_eq!(queue2.len().await, 0);
    }

    #[tokio::test]
    async fn canonicalization_failure_uses_original_path() {
        // When a path doesn't exist (canonicalize fails), the original
        // path is used as-is. Two acquires on the same non-existent path
        // should still serialize.
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("nonexistent.txt");

        let queue = FileMutationQueue::new();
        let guard1 = queue.acquire(&file).await;

        let queue_clone = queue.clone();
        let file_clone = file.clone();
        let handle = tokio::spawn(async move {
            let _g = queue_clone.acquire(&file_clone).await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(
            !handle.is_finished(),
            "should block on same non-existent path"
        );

        drop(guard1);
        tokio::time::timeout(std::time::Duration::from_secs(1), handle)
            .await
            .expect("should complete after guard drops")
            .unwrap();

        assert!(queue.is_empty().await);
    }

    #[tokio::test]
    async fn multiple_waiters_serialize() {
        // Multiple waiters on the same path are serialized: they acquire
        // the lock one at a time, in arrival order (modulo scheduler).
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("multi.txt");
        std::fs::write(&file, "").unwrap();

        let queue = FileMutationQueue::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let max_concurrent = Arc::new(AtomicUsize::new(0));
        let current = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for _ in 0..5 {
            let queue_clone = queue.clone();
            let file_clone = file.clone();
            let counter_clone = counter.clone();
            let max_clone = max_concurrent.clone();
            let current_clone = current.clone();
            handles.push(tokio::spawn(async move {
                let _g = queue_clone.acquire(&file_clone).await;
                let cur = current_clone.fetch_add(1, Ordering::SeqCst) + 1;
                max_clone.fetch_max(cur, Ordering::SeqCst);
                counter_clone.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                current_clone.fetch_sub(1, Ordering::SeqCst);
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        // All 5 tasks should have executed.
        assert_eq!(counter.load(Ordering::SeqCst), 5);
        // But never more than 1 at a time.
        assert_eq!(max_concurrent.load(Ordering::SeqCst), 1);
        assert!(queue.is_empty().await);
    }
}
