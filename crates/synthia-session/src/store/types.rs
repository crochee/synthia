//! On-disk record types and naming constants for the session store.
//!
//! These are pure data definitions — no I/O, no behavior. The disk
//! layout and the user/legacy namespace policy live here so every
//! sub-module that touches the filesystem reads the same rules.

use serde::{Deserialize, Serialize};
use synthia_provider::types::Message;

use crate::types::*;

/// Default `user_id` for a single-tenant server deployment that has
/// not been configured with a per-key mapping. The server's auth
/// middleware falls back to this id only when the operator has
/// explicitly opted into the no-auth path; production deployments
/// should configure `AuthConfig.key_to_user` instead.
///
/// The CLI REPL and the cleanup daemon MUST NOT use this constant;
/// they source `user_id` from the persisted local identity file
/// (CLI) and from per-user iteration (`Store::list_user_ids`,
/// cleanup) respectively.
pub const SERVER_DEFAULT_USER_ID: &str = "_legacy_";

/// Reserved user_id prefix that the store refuses to use for normal
/// sessions. Keeping the legacy tenant under its own prefix prevents
/// collisions with real user_ids and makes audit logs unambiguous.
pub const RESERVED_USER_ID_PREFIX: &str = "_";

pub(super) const METADATA_TEMP_SUFFIX: &str = ".tmp";

/// Session metadata stored in metadata.json.
///
/// `owner_user_id` namespaces the on-disk directory by user
/// (`{sessions_root}/{owner_user_id}/{id}/`). It is marked
/// `#[serde(default)]` so that legacy metadata files written before the
/// 2026-06-16 user_id-namespace change still deserialize; the loader
/// treats the empty string as the legacy single-tenant layout and
/// triggers an automatic migration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub version: u32,
    pub id: String,
    /// Owning user identifier. `#[serde(default)]` keeps the
    /// deserializer backward-compatible with v0 metadata.
    #[serde(default)]
    pub owner_user_id: String,
    pub state: SessionState,
    pub token_usage: TokenUsage,
    pub created_at: String,
    pub updated_at: String,
    pub config: SessionConfig,
    pub message_count: usize,
    /// Reason the session ended. `#[serde(default)]` ensures
    /// backward-compatible deserialization of legacy metadata
    /// files that do not include this field.
    #[serde(default)]
    pub end_reason: Option<String>,
    /// Current iteration count of the agent loop. Used for
    /// checkpoint-resume to restore the loop position.
    #[serde(default)]
    pub iteration: usize,
    /// Cumulative token count across all LLM calls in this session.
    #[serde(default)]
    pub cumulative_tokens: usize,
    /// Hard token limit for the agent's context window. When set,
    /// the agent will use this as an upper bound for context
    /// utilization calculations.
    #[serde(default)]
    pub context_token_limit: Option<usize>,
    /// Human-readable session title. `#[serde(default)]` keeps the
    /// deserializer backward-compatible with metadata files written
    /// before this field was introduced.
    #[serde(default)]
    pub title: Option<String>,
    /// Version of the session controller that last wrote this
    /// metadata. `#[serde(default)]` preserves backward compatibility
    /// with legacy metadata files that do not include this field.
    #[serde(default)]
    pub controller_version: u32,
    /// Optional identifier of the parent session. Populated when this
    /// session was created as a child of another session (e.g. a
    /// subagent). `#[serde(default)]` keeps the deserializer backward
    /// compatible with metadata files written before this field was
    /// introduced.
    #[serde(default)]
    pub parent_id: Option<String>,
}

/// Checkpoint data persisted to checkpoint_{step}.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointData {
    pub session_id: String,
    pub step: usize,
    pub iteration: usize,
    pub messages: Vec<Message>,
}
