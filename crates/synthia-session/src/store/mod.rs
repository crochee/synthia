//! On-disk session store: `metadata.json`, `messages.jsonl`,
//! and `checkpoint_{step}.json` under
//! `{sessions_root}/{user_id}/{session_id}/`.
//!
//! Each on-disk concern lives in its own sub-module so that the
//! public surface here stays focused on the `Store` façade:
//!
//! * [`types`]      — record types and naming constants
//! * [`dir`]        — directory layout and namespace-wide listing
//! * [`metadata`]   — `metadata.json` read/write
//! * [`messages`]   — `messages.jsonl` append/load
//! * [`events`]     — `events.jsonl` append-only event log
//! * [`checkpoint`] — `checkpoint_{step}.json` read/write
//!
//! The legacy single-tenant layout (`{sessions_root}/{session_id}/`)
//! is automatically migrated to `{sessions_root}/_legacy_/{session_id}/`
//! on the first metadata read after the upgrade. New code MUST supply
//! a non-empty `user_id` (see `Session::new_with_user`).

mod checkpoint;
mod dir;
mod events;
mod messages;
mod metadata;
mod session_input;
mod types;

#[cfg(test)]
mod tests;

use std::{fs, path::PathBuf, sync::Arc};

use anyhow::Result;
pub use events::{EventSource, EventStore, PersistedEvent};
use serde::{Deserialize, Serialize};
pub use session_input::SessionInputQueue;
pub use types::{
    CheckpointData,
    RESERVED_USER_ID_PREFIX,
    SERVER_DEFAULT_USER_ID,
    SessionMetadata,
};

use crate::types::Session;

/// `Store` handles persistence of session data to disk.
///
/// Each session is stored under: `{sessions_root}/{user_id}/{session_id}/`
/// - metadata.json: session metadata
/// - messages.jsonl: append-only message log
#[derive(Clone)]
pub struct Store {
    sessions_root: PathBuf,
    event_store: Arc<EventStore>,
}

impl Store {
    pub fn new(sessions_root: PathBuf) -> Self {
        Self {
            sessions_root,
            event_store: Arc::new(EventStore::new()),
        }
    }

    /// Returns the absolute path of the sessions root. Exposed as
    /// `pub(crate)` so that privileged callers in the manager (e.g.
    /// `list_persisted_sessions`) can iterate the raw tree; normal
    /// callers MUST go through the per-user `list_session_ids` API
    /// instead.
    pub(crate) fn sessions_root_path(&self) -> &PathBuf {
        &self.sessions_root
    }

    /// Returns the shared `EventStore` for this session store.
    ///
    /// All clones of `Store` share the same `EventStore` instance,
    /// so the seq cache is reused across calls.
    pub fn event_store(&self) -> &EventStore {
        &self.event_store
    }

    /// Returns the directory path for a given session under the
    /// `user_id` namespace.
    ///
    /// Layout: `{sessions_root}/{user_id}/{session_id}/`
    pub fn session_dir(&self, user_id: &str, session_id: &str) -> PathBuf {
        dir::session_path(&self.sessions_root, user_id, session_id)
    }

    /// Returns the directory holding all sessions for a given user.
    pub fn user_dir(&self, user_id: &str) -> PathBuf {
        dir::user_path(&self.sessions_root, user_id)
    }

    /// Ensure the session directory exists. Sets 0o700 permissions on
    /// Unix so that only the owning user can read or modify the
    /// session's contents; the user_id namespace prevents other users
    /// from reaching this directory at all.
    pub fn ensure_session_dir(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Result<PathBuf> {
        let dir = self.session_dir(user_id, session_id);
        dir::ensure_session_dir(&dir)
    }

    /// List all user_ids that currently have a session directory
    /// under `sessions_root`. See [`dir::list_user_ids`] for the
    /// full contract.
    pub fn list_user_ids(&self) -> Result<Vec<String>> {
        dir::list_user_ids(&self.sessions_root)
    }

    /// Save session metadata to metadata.json in the session directory.
    /// Uses atomic write (temp file + rename) to prevent corruption.
    pub fn save_metadata(&self, session: &Session) -> Result<()> {
        let dir = self.ensure_session_dir(&session.user_id, &session.id)?;
        metadata::save_to(&dir, session)
    }

    /// Load session metadata from metadata.json under the given
    /// `user_id` namespace.
    pub fn load_metadata(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Result<SessionMetadata> {
        metadata::load_from(&self.sessions_root, user_id, session_id)
    }

    /// Append a serialized message to messages.jsonl.
    /// Uses fsync to ensure durability.
    pub fn append_message_raw(
        &self,
        user_id: &str,
        session_id: &str,
        message_json: &str,
    ) -> Result<()> {
        let dir = self.ensure_session_dir(user_id, session_id)?;
        messages::append_raw(&dir, message_json)
    }

    /// Append a message to the session's messages.jsonl file.
    /// Accepts any type that can be serialized to JSON.
    pub fn append_message(
        &self,
        user_id: &str,
        session_id: &str,
        message: &impl Serialize,
    ) -> Result<()> {
        let dir = self.ensure_session_dir(user_id, session_id)?;
        messages::append(&dir, message)
    }

    /// Load the most recent N messages from messages.jsonl using a read-from-end
    /// strategy. This is more efficient for large session files as it avoids
    /// loading the entire file into memory.
    ///
    /// Returns messages in chronological order (oldest first).
    /// Default limit of 100 is used for typical session interactions.
    pub fn load_messages_recent<T>(
        &self,
        user_id: &str,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<T>>
    where
        T: for<'de> Deserialize<'de>,
    {
        let dir = self.session_dir(user_id, session_id);
        messages::load_recent(&dir, limit)
    }

    /// Load all messages from messages.jsonl.
    /// Used for compaction scenarios where full history is needed.
    pub fn load_messages_all<T>(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Result<Vec<T>>
    where
        T: for<'de> Deserialize<'de>,
    {
        let dir = self.session_dir(user_id, session_id);
        messages::load_all(&dir)
    }

    /// Load messages from the beginning of the file up to (but not including)
    /// the messages already loaded via `load_messages_recent`.
    ///
    /// `skip_count` is the number of most recent messages already in memory.
    /// `limit` controls how many older messages to load (use usize::MAX for all).
    ///
    /// Returns messages in chronological order (oldest first).
    pub fn load_messages_older_than<T>(
        &self,
        user_id: &str,
        session_id: &str,
        skip_count: usize,
        limit: usize,
    ) -> Result<Vec<T>>
    where
        T: for<'de> Deserialize<'de>,
    {
        let dir = self.session_dir(user_id, session_id);
        messages::load_older_than(&dir, skip_count, limit)
    }

    /// Save checkpoint data for session.
    pub fn save_checkpoint(
        &self,
        user_id: &str,
        session_id: &str,
        data: &CheckpointData,
    ) -> Result<()> {
        let dir = self.ensure_session_dir(user_id, session_id)?;
        checkpoint::save_to(&dir, data)
    }

    /// Load latest checkpoint for session.
    pub fn load_latest_checkpoint(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Result<Option<CheckpointData>> {
        let dir = self.session_dir(user_id, session_id);
        checkpoint::load_latest_from(&dir)
    }

    /// Delete the entire session directory including all files.
    pub fn delete_session(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Result<()> {
        let dir = self.session_dir(user_id, session_id);
        if dir.exists() {
            fs::remove_dir_all(dir)?;
        }
        Ok(())
    }

    /// Check if a session directory exists on disk.
    pub fn session_exists(&self, user_id: &str, session_id: &str) -> bool {
        self.session_dir(user_id, session_id).exists()
    }

    /// List all session ids under the given user's namespace.
    ///
    /// Does NOT list other users' sessions: each user has their own
    /// directory and the store does not cross user_id boundaries.
    pub fn list_session_ids(&self, user_id: &str) -> Result<Vec<String>> {
        dir::list_session_ids(&self.sessions_root, user_id)
    }

    /// List sessions visible to `caller_user_id`, returning their
    /// metadata. See [`dir::list_sessions_with_metadata`] for the
    /// full contract.
    pub fn list_sessions_with_metadata(
        &self,
        caller_user_id: &str,
    ) -> Result<Vec<SessionMetadata>> {
        dir::list_sessions_with_metadata(&self.sessions_root, caller_user_id)
    }
}

// R3: re-export `synthia-session-v2` types under the `store::*` path so
// new code can reach them without going through the crate root. The
// legacy `Store` / `SessionMetadata` / `CheckpointData` types above stay
// intact for `manager/`, `service.rs`, and `state_machine/` callers
// until those callers are ported to v2.
#[allow(deprecated)]
pub use synthia_session_v2::{
    AgentPart,
    AttachmentRef,
    CURRENT_SESSION_VERSION,
    CompactionPart,
    FilePart,
    Message,
    MessageError,
    MessageInfo,
    MessageTime,
    Part,
    PatchPart,
    ReasoningPart,
    Role,
    SessionEntry,
    SessionTree,
    SnapshotPart,
    StepFinishPart,
    StepStartPart,
    SubtaskPart,
    TextPart,
    ToolPart,
    ToolState,
    ToolTime,
};
