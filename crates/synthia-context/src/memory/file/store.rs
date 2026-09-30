//! [`FileMemory`] — the persistent, scoped long-term tier.

use std::{cmp::Reverse, fmt, fs, path::Path, sync::Arc};

use async_trait::async_trait;
use parking_lot::Mutex;
use synthia_provider::Message;
use ulid::Ulid;

use super::{
    ENTRIES_DIR,
    INDEX_FILE,
    MAX_MEMORY_INDEX_LINES,
    config::{FileMemoryConfig, ResolvedScope},
    entry::{FileMemoryEntry, render_entry},
    error::FileMemoryError,
    guards::{check_dir_below, io_error, refuse_symlink, validate_agent},
    index::{FoundEntry, render_index, scan_dir, score, tokenize},
    scope::MemoryScope,
};
use crate::memory::{InMemoryMemory, Memory, MemoryEntry, MemoryError};

/// Persistent, scoped long-term memory over a directory tree.
///
/// Entries are one file per ULID under each scope's `entries/`
/// directory; `MEMORY.md` in the primary scope is a bounded index
/// regenerated on every write. See the `memory::file` module docs
/// for the scope layout, the index contract, and the recall
/// ranking.
pub struct FileMemory {
    config: FileMemoryConfig,
    /// Resolved scope directories in precedence order.
    dirs: Vec<ResolvedScope>,
    /// Conversation and working tiers.
    inner: Arc<dyn Memory>,
    /// Serialises entry writes plus index regeneration.
    write_lock: Mutex<()>,
}

impl fmt::Debug for FileMemory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FileMemory")
            .field("root", &self.config.root)
            .field("agent", &self.config.agent)
            .field("scopes", &self.config.scopes)
            .field("read_only", &self.config.read_only)
            .finish_non_exhaustive()
    }
}

impl FileMemory {
    /// Build a file-backed tier whose conversation and working
    /// memory live in an [`InMemoryMemory`].
    ///
    /// # Errors
    ///
    /// Returns [`FileMemoryError::InvalidAgentName`] for a name
    /// outside the whitelist, [`FileMemoryError::NoScopes`] when no
    /// scope is configured, and [`FileMemoryError::MissingHome`]
    /// when the `user` scope has no anchor.
    pub fn new(config: FileMemoryConfig) -> Result<Self, FileMemoryError> {
        Self::with_inner(config, Arc::new(InMemoryMemory::new()))
    }

    /// Build a file-backed tier over a caller-supplied inner
    /// [`Memory`] (e.g. a sink-backed `SessionMemory`), so the
    /// durable long-term tier and the volatile tiers compose.
    ///
    /// # Errors
    ///
    /// Same as [`FileMemory::new`].
    pub fn with_inner(
        config: FileMemoryConfig,
        inner: Arc<dyn Memory>,
    ) -> Result<Self, FileMemoryError> {
        validate_agent(&config.agent)?;
        if config.scopes.is_empty() {
            return Err(FileMemoryError::NoScopes);
        }
        let home = if config.scopes.contains(&MemoryScope::User) {
            config.home()?
        } else {
            config.root.clone()
        };
        let mut dirs = Vec::with_capacity(config.scopes.len());
        for scope in &config.scopes {
            dirs.push(ResolvedScope::resolve(*scope, &config, &home));
        }
        Ok(Self {
            config,
            dirs,
            inner,
            write_lock: Mutex::new(()),
        })
    }

    /// Configuration this tier was built from.
    #[must_use]
    pub fn config(&self) -> &FileMemoryConfig {
        &self.config
    }

    /// Resolved directory of `scope`, or `None` when the scope is
    /// not configured.
    #[must_use]
    pub fn dir(&self, scope: MemoryScope) -> Option<&Path> {
        self.dirs
            .iter()
            .find(|resolved| resolved.scope == scope)
            .map(|resolved| resolved.dir.as_path())
    }

    /// `true` when writes are refused.
    #[must_use]
    pub fn is_read_only(&self) -> bool {
        self.config.read_only
    }

    /// Write one entry into the primary scope and refresh its
    /// index, returning the minted ULID.
    ///
    /// # Errors
    ///
    /// Returns [`FileMemoryError::ReadOnly`] in read-only mode,
    /// [`FileMemoryError::Symlink`] for a symlinked path, and
    /// [`FileMemoryError::Io`] for filesystem failures.
    pub fn store_entry(
        &self,
        entry: &FileMemoryEntry,
    ) -> Result<String, FileMemoryError> {
        let id = self.store_at(Ulid::generate(), entry)?;
        Ok(id.to_string())
    }

    /// Search every configured scope for `query`, best hit first.
    ///
    /// An empty query matches every entry; `limit` truncates the
    /// ranked result (`limit == 0` returns nothing).
    ///
    /// # Errors
    ///
    /// Returns [`FileMemoryError::Symlink`] or
    /// [`FileMemoryError::Io`] when a scope directory cannot be
    /// read. Never creates directories.
    pub fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>, FileMemoryError> {
        let tokens = tokenize(query);
        let mut seen: Vec<Ulid> = Vec::new();
        let mut scored: Vec<(usize, MemoryEntry)> = Vec::new();
        for resolved in &self.dirs {
            for found in scan_dir(resolved)? {
                if seen.contains(&found.id) {
                    continue;
                }
                seen.push(found.id);
                let rank = if tokens.is_empty() {
                    1
                } else {
                    score(&found, &tokens)
                };
                if rank == 0 {
                    continue;
                }
                scored.push((rank, found.into_memory_entry(resolved.scope)));
            }
        }
        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.id.cmp(&a.1.id)));
        scored.truncate(limit);
        Ok(scored.into_iter().map(|(_, entry)| entry).collect())
    }

    /// The bounded index injected into prompts: one line per entry
    /// across every configured scope, newest first, hard-capped at
    /// [`MAX_MEMORY_INDEX_LINES`] lines.
    ///
    /// # Errors
    ///
    /// Returns [`FileMemoryError::Symlink`] or
    /// [`FileMemoryError::Io`] when a scope directory cannot be
    /// read.
    pub fn index(&self) -> Result<String, FileMemoryError> {
        Ok(render_index(&self.load_merged()?))
    }

    /// Prompt block for the memory tier: scope, directory, current
    /// index, and the write instructions (or the read-only
    /// variant). Reads only; a read-only tier never creates the
    /// memory directory.
    ///
    /// # Errors
    ///
    /// Returns [`FileMemoryError::Symlink`] or
    /// [`FileMemoryError::Io`] when a scope directory cannot be
    /// read.
    pub fn prompt_block(&self) -> Result<String, FileMemoryError> {
        let primary = self.primary()?;
        let merged = self.load_merged()?;
        let mut block = String::new();
        if self.config.read_only {
            block.push_str("# Agent Memory (read-only)\n\n");
            block.push_str(&format!(
                "Memory directory: {}\nMemory scope: {}\n",
                primary.dir.display(),
                primary.scope.as_str(),
            ));
            block.push_str(
                "You have read-only access to memory: reference existing \
                 memories, but do not create or modify them.\n",
            );
        } else {
            block.push_str("# Agent Memory\n\n");
            block.push_str(&format!(
                "You have a persistent memory directory at: {}\nMemory scope: {}\n",
                primary.dir.display(),
                primary.scope.as_str(),
            ));
            block.push_str(
                "This memory persists across sessions. Use it to build up \
                 knowledge over time.\n",
            );
        }
        block.push_str("\n## Current MEMORY.md\n");
        if merged.is_empty() {
            block.push_str("No memories stored yet.\n");
        } else {
            block.push_str(&render_index(&merged));
        }
        if !self.config.read_only {
            block.push_str("\n## Memory Instructions\n");
            block.push_str(&format!(
                "- {} is a generated index of every entry; do not edit it by \
                 hand (it is rewritten on every write and bounded to {} \
                 lines).\n",
                INDEX_FILE, MAX_MEMORY_INDEX_LINES,
            ));
            block.push_str(
                "- Each entry is one file under `entries/<ulid>.md` with \
                 `name`, `description`, and `type` \
                 (user|feedback|project|reference) frontmatter plus a \
                 detail body.\n",
            );
            block.push_str(
                "- Update or remove entries that become outdated, and \
                 check for a duplicate before storing a new one.\n",
            );
        }
        Ok(block)
    }

    /// Write `entry` under `id`, then re-render the primary scope's
    /// index from disk.
    fn store_at(
        &self,
        id: Ulid,
        entry: &FileMemoryEntry,
    ) -> Result<Ulid, FileMemoryError> {
        let target = self.primary()?;
        if self.config.read_only {
            return Err(FileMemoryError::ReadOnly(target.dir.clone()));
        }
        let _guard = self.write_lock.lock();
        let entries_dir = target.dir.join(ENTRIES_DIR);
        check_dir_below(
            &target.base,
            &target.relative.join(ENTRIES_DIR),
            true,
        )?;
        let path = entries_dir.join(format!("{id}.md"));
        refuse_symlink(&path)?;
        fs::write(&path, render_entry(entry))
            .map_err(|error| io_error(&path, error))?;
        let entries = scan_dir(target)?;
        let index_path = target.dir.join(INDEX_FILE);
        refuse_symlink(&index_path)?;
        fs::write(&index_path, render_index(&entries))
            .map_err(|error| io_error(&index_path, error))?;
        Ok(id)
    }

    /// Primary scope: the write target and the scope the prompt
    /// block advertises.
    fn primary(&self) -> Result<&ResolvedScope, FileMemoryError> {
        self.dirs.first().ok_or(FileMemoryError::NoScopes)
    }

    /// Every entry across the configured scopes, deduplicated by id
    /// (nearer scope wins) and sorted newest first.
    fn load_merged(&self) -> Result<Vec<FoundEntry>, FileMemoryError> {
        let mut seen: Vec<Ulid> = Vec::new();
        let mut merged: Vec<FoundEntry> = Vec::new();
        for resolved in &self.dirs {
            for found in scan_dir(resolved)? {
                if seen.contains(&found.id) {
                    continue;
                }
                seen.push(found.id);
                merged.push(found);
            }
        }
        merged.sort_by_key(|entry| Reverse(entry.id));
        Ok(merged)
    }
}

#[async_trait]
impl Memory for FileMemory {
    async fn messages(
        &self,
        session_id: &str,
    ) -> Result<Vec<Message>, MemoryError> {
        self.inner.messages(session_id).await
    }

    async fn append(
        &self,
        session_id: &str,
        message: Message,
    ) -> Result<(), MemoryError> {
        self.inner.append(session_id, message).await
    }

    async fn get_context(
        &self,
        session_id: &str,
        key: &str,
    ) -> Result<Option<serde_json::Value>, MemoryError> {
        self.inner.get_context(session_id, key).await
    }

    async fn set_context(
        &self,
        session_id: &str,
        key: &str,
        value: serde_json::Value,
    ) -> Result<(), MemoryError> {
        self.inner.set_context(session_id, key, value).await
    }

    async fn recall(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>, MemoryError> {
        self.search(query, limit).map_err(MemoryError::from)
    }

    async fn store(&self, entry: MemoryEntry) -> Result<(), MemoryError> {
        // A caller-supplied ULID is honoured so a specific entry can
        // be re-stored (which replaces it); any other id is replaced
        // by a fresh ULID, since file names are entry identity.
        let id =
            Ulid::from_string(&entry.id).unwrap_or_else(|_| Ulid::generate());
        let file_entry = FileMemoryEntry::from_memory_entry(&entry);
        self.store_at(id, &file_entry)
            .map(|_| ())
            .map_err(MemoryError::from)
    }

    async fn create_session(&self) -> Result<String, MemoryError> {
        self.inner.create_session().await
    }

    async fn list_sessions(&self) -> Result<Vec<String>, MemoryError> {
        self.inner.list_sessions().await
    }

    async fn delete_session(
        &self,
        session_id: &str,
    ) -> Result<(), MemoryError> {
        self.inner.delete_session(session_id).await
    }
}
