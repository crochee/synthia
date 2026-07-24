//! [`AgentFileLoader`] — the in-process cache +
//! change-event queue + filesystem watcher.
//!
//! The struct holds:
//!
//! - `base_path`: the directory under which
//!   `<id>.md` files live.
//! - `cache`: `RwLock<HashMap<id, (frontmatter,
//!   body, sha256)>>` — the parsed-content cache
//!   that makes the second `load(id)` call a
//!   no-filesystem-touch op. The SHA-256 is the
//!   dirty-check key for [`Self::reload`].
//! - `change_events`: `RwLock<Vec<AgentChangeEvent>>`
//!   — the queue [`Self::take_change_events`]
//!   drains. The `watch` method below uses a
//!   **separate** mpsc channel for its background
//!   events; the in-struct queue is only for the
//!   explicit `reload` / `detect_removals` paths.
//!
//! ## Public method map
//!
//! - [`new`](Self::new): construct an empty
//!   loader.
//! - [`list_ids`](Self::list_ids): enumerate the
//!   `.md` stems under `base_path`. **No cache**,
//!   **no frontmatter parse** — just
//!   `read_dir` + `trim_end_matches(".md")`.
//! - [`load`](Self::load): cache-first read. On
//!   cache hit, returns the cached parsed file.
//!   On miss, reads from disk, parses the
//!   frontmatter, hashes the content, and (if the
//!   file has frontmatter) inserts the cache
//!   entry and queues an `Added` event.
//! - [`take_change_events`](Self::take_change_events):
//!   drain the event queue.
//! - [`reload`](Self::reload): re-read the file
//!   from disk, hash, compare to the cached
//!   hash. Queues `Modified` if the hash
//!   changed, `Added` if the id wasn't cached
//!   yet, nothing if unchanged.
//! - [`detect_removals`](Self::detect_removals):
//!   scan the cache for ids whose `.md` file is
//!   gone from disk; drop them from the cache
//!   and queue `Removed` events.
//! - [`watch`](Self::watch): build a
//!   `RecommendedWatcher` (500ms poll) that
//!   forwards `.md` events on `base_path` to an
//!   mpsc channel. The watcher is returned to
//!   the caller; the receiver is dropped here
//!   (the watcher's lifetime is the caller's).

use std::{collections::HashMap, path::PathBuf, sync::RwLock, time::Duration};

use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;

use super::types::AgentChangeEvent;
use crate::agent_file::{
    frontmatter::FileAgentFrontmatter,
    parser::{ParsedAgentFile, split_frontmatter},
};

/// In-process agent-file cache + event queue +
/// filesystem watcher. See the module-level
/// rustdoc for the full method map.
pub struct AgentFileLoader {
    /// Directory under which `<id>.md` files live.
    base_path: PathBuf,
    /// `id -> (frontmatter, body, content_sha256)`.
    /// The SHA-256 is the dirty-check key for
    /// [`Self::reload`].
    cache: RwLock<HashMap<String, (FileAgentFrontmatter, String, String)>>,
    /// Queue [`Self::take_change_events`] drains.
    /// Mutated by `load` / `reload` /
    /// `detect_removals`; the `watch` method uses
    /// its own mpsc channel.
    change_events: RwLock<Vec<AgentChangeEvent>>,
}

impl AgentFileLoader {
    /// Build an empty loader rooted at
    /// `base_path`.
    pub fn new(base_path: PathBuf) -> Self {
        Self {
            base_path,
            cache: RwLock::new(HashMap::new()),
            change_events: RwLock::new(Vec::new()),
        }
    }

    /// Enumerate the `.md` stems under
    /// `base_path`. No cache, no frontmatter
    /// parse — just `read_dir` +
    /// `trim_end_matches(".md")`.
    pub fn list_ids(&self) -> Vec<String> {
        let mut ids = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.base_path) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    let id = name.trim_end_matches(".md");
                    if !id.is_empty() && name.ends_with(".md") {
                        ids.push(id.to_string());
                    }
                }
            }
        }
        ids
    }

    /// Cache-first read. On hit, returns the
    /// cached parsed file. On miss, reads from
    /// disk, parses, hashes, and (if frontmatter
    /// is present) inserts the cache entry +
    /// queues an `Added` event.
    pub fn load(&self, id: &str) -> Result<ParsedAgentFile, String> {
        // Check cache first - serve from cache if available
        if let Ok(cache) = self.cache.read()
            && let Some((fm, body, _)) = cache.get(id)
        {
            return Ok(ParsedAgentFile {
                frontmatter: Some(fm.clone()),
                body: body.clone(),
            });
        }

        // Not in cache, read from disk
        let path = self.base_path.join(format!("{}.md", id));
        let content = std::fs::read_to_string(&path).map_err(|e| {
            format!("failed to read '{}': {}", path.display(), e)
        })?;
        let content_hash = {
            let mut hasher = Sha256::new();
            hasher.update(content.as_bytes());
            format!("{:x}", hasher.finalize())
        };
        let parsed = split_frontmatter(&content)?;

        if let Some(fm) = parsed.frontmatter.clone() {
            if let Ok(mut cache) = self.cache.write() {
                cache.insert(
                    id.to_string(),
                    (fm, parsed.body.clone(), content_hash),
                );
            }
            // First time loading this id, emit Added
            if let Ok(mut events) = self.change_events.write() {
                events.push(AgentChangeEvent::Added(id.to_string()));
            }
        }
        Ok(parsed)
    }

    /// Drain and return all pending change events.
    pub fn take_change_events(&self) -> Vec<AgentChangeEvent> {
        if let Ok(mut events) = self.change_events.write() {
            std::mem::take(&mut *events)
        } else {
            Vec::new()
        }
    }

    /// Re-read a file from disk, comparing with
    /// the cached version. Emits `Modified` if
    /// the content hash changed, `Added` if the
    /// id wasn't cached yet, nothing if
    /// unchanged.
    pub fn reload(&self, id: &str) -> Result<ParsedAgentFile, String> {
        let path = self.base_path.join(format!("{}.md", id));
        let content = std::fs::read_to_string(&path).map_err(|e| {
            format!("failed to read '{}': {}", path.display(), e)
        })?;
        let content_hash = {
            let mut hasher = Sha256::new();
            hasher.update(content.as_bytes());
            format!("{:x}", hasher.finalize())
        };
        let parsed = split_frontmatter(&content)?;

        if let Some(fm) = parsed.frontmatter.clone()
            && let Ok(mut cache) = self.cache.write()
        {
            if let Some((_, _, old_hash)) = cache.get(id) {
                if old_hash != &content_hash {
                    // Content changed
                    cache.insert(
                        id.to_string(),
                        (fm, parsed.body.clone(), content_hash),
                    );
                    if let Ok(mut events) = self.change_events.write() {
                        events.push(AgentChangeEvent::Modified(id.to_string()));
                    }
                }
                // else: unchanged, no event
            } else {
                // Not in cache yet
                cache.insert(
                    id.to_string(),
                    (fm, parsed.body.clone(), content_hash),
                );
                if let Ok(mut events) = self.change_events.write() {
                    events.push(AgentChangeEvent::Added(id.to_string()));
                }
            }
        }
        Ok(parsed)
    }

    /// Detect files that were previously loaded
    /// but no longer exist on disk. Emits
    /// `Removed` events for each missing file and
    /// clears them from cache.
    pub fn detect_removals(&self) -> Vec<String> {
        let mut removed = Vec::new();
        if let Ok(mut cache) = self.cache.write() {
            let missing_ids: Vec<String> = cache
                .keys()
                .filter(|id| {
                    !self.base_path.join(format!("{}.md", id)).exists()
                })
                .cloned()
                .collect();
            for id in &missing_ids {
                cache.remove(id);
                removed.push(id.clone());
            }
        }
        if let Ok(mut events) = self.change_events.write() {
            for id in &removed {
                events.push(AgentChangeEvent::Removed(id.clone()));
            }
        }
        removed
    }

    /// Watch for file changes with 500ms
    /// debounce.
    ///
    /// Returns a [`RecommendedWatcher`] that,
    /// when kept alive, watches `self.base_path`
    /// non-recursively and forwards markdown file
    /// events to a tokio mpsc channel. The poll
    /// interval is 500ms.
    ///
    /// Callers typically move the watcher into a
    /// long-lived task and consume events from
    /// the receiver.
    pub fn watch(&self) -> Result<RecommendedWatcher, notify::Error> {
        let base_path = self.base_path.clone();
        let (tx, _rx) = mpsc::channel::<AgentChangeEvent>(100);

        let mut watcher = RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                let event = match res {
                    Ok(ev) => ev,
                    Err(_) => return,
                };
                for path in event.paths {
                    let Some(name) = path.file_name().and_then(|n| n.to_str())
                    else {
                        continue;
                    };
                    if !name.ends_with(".md") {
                        continue;
                    }
                    let id = name.trim_end_matches(".md").to_string();
                    if id.is_empty() {
                        continue;
                    }
                    let kind = match event.kind {
                        notify::EventKind::Create(_) => {
                            AgentChangeEvent::Added(id)
                        }
                        notify::EventKind::Modify(_) => {
                            AgentChangeEvent::Modified(id)
                        }
                        notify::EventKind::Remove(_) => {
                            AgentChangeEvent::Removed(id)
                        }
                        _ => continue,
                    };
                    let _ = tx.blocking_send(kind);
                }
            },
            Config::default().with_poll_interval(Duration::from_millis(500)),
        )?;

        watcher.watch(&base_path, RecursiveMode::NonRecursive)?;
        Ok(watcher)
    }
}
