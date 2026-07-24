//! Persisted steering input queue backed by `session_input.jsonl`.
//!
//! Each entry is a JSON line with a `consumed` flag. The queue is
//! append-only: `push` adds new entries, `drain_pending` reads and
//! marks all unconsumed entries as consumed, returning them in
//! priority-descending / timestamp-ascending order.

use std::{
    fs,
    io::{self, BufRead, BufReader, Write},
    path::PathBuf,
};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A single queued input entry stored on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedInput {
    pub content: String,
    pub priority: u8,
    pub timestamp: DateTime<Utc>,
    pub consumed: bool,
}

/// A filesystem-backed queue for steering / user input messages
/// that must survive process restarts.
///
/// The queue is rooted at `sessions_root`; each session gets its
/// own `session_input.jsonl` under `{user_id}/{session_id}/`.
#[derive(Debug, Clone)]
pub struct SessionInputQueue {
    sessions_root: PathBuf,
}

impl SessionInputQueue {
    pub fn new(sessions_root: PathBuf) -> Self {
        Self { sessions_root }
    }

    /// Build the path to `session_input.jsonl` for a given session.
    fn input_path(&self, user_id: &str, session_id: &str) -> PathBuf {
        self.sessions_root
            .join(user_id)
            .join(session_id)
            .join("session_input.jsonl")
    }

    /// Serialize a queue entry to a single JSON line.
    fn serialize_entry(entry: &QueuedInput) -> io::Result<String> {
        serde_json::to_string(entry)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// Atomically rewrite the JSONL file with the provided entries.
    ///
    /// Writes to a sibling `.tmp` file, fsyncs it, then renames it over
    /// the target so readers never see a partially-written file.
    fn atomic_rewrite_jsonl(
        &self,
        path: &std::path::Path,
        entries: &[QueuedInput],
    ) -> Result<()> {
        let tmp_path = path.with_extension("jsonl.tmp");
        let write_result = (|| -> io::Result<()> {
            let mut file = fs::File::create(&tmp_path)
                .with_context(|| {
                    format!("Failed to create temp file {:?}", tmp_path)
                })
                .map_err(io::Error::other)?;
            for entry in entries {
                let json = Self::serialize_entry(entry)?;
                writeln!(file, "{json}")?;
            }
            file.sync_all()?;
            fs::rename(&tmp_path, path)?;
            Ok(())
        })();

        if write_result.is_err() {
            // Best-effort cleanup of the temporary file; the original
            // file is untouched if the rename never happened.
            let _ = fs::remove_file(&tmp_path);
        }
        write_result
            .with_context(|| format!("Failed to rewrite {:?}", path))?;
        Ok(())
    }

    /// Append a new steering / user input message to the session's
    /// input queue. The entry is written with `consumed: false`.
    pub fn push(
        &self,
        user_id: &str,
        session_id: &str,
        content: String,
        priority: u8,
    ) -> Result<()> {
        let entry = QueuedInput {
            content,
            priority,
            timestamp: Utc::now(),
            consumed: false,
        };
        let json = Self::serialize_entry(&entry)?;
        let path = self.input_path(user_id, session_id);
        if let Some(parent) = path.parent() {
            let existed = parent.exists();
            fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create dir {:?}", parent)
            })?;
            if !existed {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(
                        parent,
                        fs::Permissions::from_mode(0o700),
                    )
                    .with_context(|| {
                        format!("Failed to set 0o700 on {:?}", parent)
                    })?;
                }
            }
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("Failed to open {:?}", path))?;
        writeln!(file, "{json}")?;
        file.sync_all()
            .with_context(|| format!("Failed to sync {:?}", path))?;
        Ok(())
    }

    /// Read all unconsumed entries, mark them as consumed, and return
    /// them sorted by priority (descending) then timestamp (ascending).
    pub fn drain_pending(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Result<Vec<QueuedInput>> {
        let path = self.input_path(user_id, session_id);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let file = fs::File::open(&path)
            .with_context(|| format!("Failed to open {:?}", path))?;
        let reader = BufReader::new(file);
        let mut entries: Vec<QueuedInput> = Vec::new();
        let mut pending: Vec<QueuedInput> = Vec::new();
        for (line_no, line) in reader.lines().enumerate() {
            let line = line?;
            match serde_json::from_str::<QueuedInput>(&line) {
                Ok(mut input) => {
                    if !input.consumed {
                        pending.push(input.clone());
                        input.consumed = true;
                    }
                    entries.push(input);
                }
                Err(e) => {
                    tracing::warn!(
                        line = line_no + 1,
                        error = %e,
                        path = %path.display(),
                        "Skipping malformed session input line"
                    );
                }
            }
        }
        // Sort pending: highest priority first, then earliest timestamp.
        pending.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| a.timestamp.cmp(&b.timestamp))
        });

        // Rewrite the file with all entries marked consumed.
        if !pending.is_empty() {
            self.atomic_rewrite_jsonl(&path, &entries)?;
        }
        Ok(pending)
    }

    /// Check whether the session has any unconsumed input entries.
    pub fn has_pending(&self, user_id: &str, session_id: &str) -> bool {
        let path = self.input_path(user_id, session_id);
        if !path.exists() {
            return false;
        }
        let Ok(file) = fs::File::open(&path) else {
            return false;
        };
        let reader = BufReader::new(file);
        for (line_no, line) in reader.lines().enumerate() {
            let Ok(line) = line else {
                continue;
            };
            match serde_json::from_str::<QueuedInput>(&line) {
                Ok(input) => {
                    if !input.consumed {
                        return true;
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        line = line_no + 1,
                        error = %e,
                        path = %path.display(),
                        "Skipping malformed session input line"
                    );
                }
            }
        }
        false
    }

    /// Boost the priority of all unconsumed entries whose content
    /// contains `content_prefix` to `u8::MAX` (255).
    pub fn promote(
        &self,
        user_id: &str,
        session_id: &str,
        content_prefix: &str,
    ) -> Result<()> {
        let path = self.input_path(user_id, session_id);
        if !path.exists() {
            return Ok(());
        }
        let file = fs::File::open(&path)
            .with_context(|| format!("Failed to open {:?}", path))?;
        let reader = BufReader::new(file);
        let mut entries: Vec<QueuedInput> = Vec::new();
        let mut modified = false;
        for (line_no, line) in reader.lines().enumerate() {
            let line = line?;
            match serde_json::from_str::<QueuedInput>(&line) {
                Ok(mut input) => {
                    if !input.consumed && input.content.contains(content_prefix)
                    {
                        input.priority = u8::MAX;
                        modified = true;
                    }
                    entries.push(input);
                }
                Err(e) => {
                    tracing::warn!(
                        line = line_no + 1,
                        error = %e,
                        path = %path.display(),
                        "Skipping malformed session input line"
                    );
                }
            }
        }
        if modified {
            self.atomic_rewrite_jsonl(&path, &entries)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_push_persists_to_disk() {
        let tmp = TempDir::new().unwrap();
        let queue = SessionInputQueue::new(tmp.path().to_path_buf());
        queue
            .push("user-1", "sess-1", "hello".to_string(), 5)
            .unwrap();

        // Read the file directly and verify content is flushed.
        let path = queue.input_path("user-1", "sess-1");
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("hello"));
        assert!(content.contains("\"consumed\":false"));
    }

    #[test]
    fn test_drain_pending_persists_consumed_markers() {
        let tmp = TempDir::new().unwrap();
        let queue = SessionInputQueue::new(tmp.path().to_path_buf());
        queue
            .push("user-1", "sess-1", "first".to_string(), 5)
            .unwrap();
        queue
            .push("user-1", "sess-1", "second".to_string(), 5)
            .unwrap();

        let drained = queue.drain_pending("user-1", "sess-1").unwrap();
        assert_eq!(drained.len(), 2);

        // Re-read file: all entries should now be marked consumed=true.
        let path = queue.input_path("user-1", "sess-1");
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(
            !content.contains("\"consumed\":false"),
            "all entries should be marked consumed after drain_pending"
        );
    }

    #[test]
    fn test_promote_persists_priority_changes() {
        let tmp = TempDir::new().unwrap();
        let queue = SessionInputQueue::new(tmp.path().to_path_buf());
        queue
            .push("user-1", "sess-1", "urgent-msg".to_string(), 1)
            .unwrap();

        queue.promote("user-1", "sess-1", "urgent").unwrap();

        let path = queue.input_path("user-1", "sess-1");
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(
            content.contains("\"priority\":255"),
            "promote should persist priority=255"
        );
    }
}
