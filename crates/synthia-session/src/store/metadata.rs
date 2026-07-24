//! `metadata.json` read/write. Atomic write (temp file + rename +
//! fsync) prevents corruption on crash.

use std::{fs, io::Write, path::Path};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};

use super::{
    dir::ensure_session_dir,
    types::{METADATA_TEMP_SUFFIX, SessionMetadata},
};
use crate::{error::StoreError, types::Session};

/// Write `metadata.json` atomically under `dir`. The caller is
/// responsible for ensuring the session directory exists.
pub(crate) fn save_to(dir: &Path, session: &Session) -> Result<()> {
    if session.user_id.is_empty() {
        return Err(StoreError::EmptyUserId {
            session_id: session.id.clone(),
        }
        .into());
    }
    ensure_session_dir(dir)?;
    let message_count = super::dir::count_messages_in(dir)?;
    let metadata = SessionMetadata {
        version: 1,
        id: session.id.clone(),
        owner_user_id: session.user_id.clone(),
        state: session.state,
        token_usage: session.token_usage.clone(),
        created_at: format_timestamp_utc(&session.created_at),
        updated_at: format_timestamp_utc(&session.updated_at),
        config: session.config.clone(),
        message_count,
        end_reason: session.end_reason.clone(),
        iteration: session.iteration,
        cumulative_tokens: session.cumulative_tokens,
        context_token_limit: session.context_token_limit,
        title: None,
        controller_version: 1,
        parent_id: session.parent_id.clone(),
    };

    let json = serde_json::to_string_pretty(&metadata)?;
    let path = dir.join("metadata.json");
    let temp_path = dir.join(format!("metadata{METADATA_TEMP_SUFFIX}"));

    let mut file = fs::File::create(&temp_path).with_context(|| {
        format!("Failed to create temp file: {:?}", temp_path)
    })?;
    file.write_all(json.as_bytes()).with_context(|| {
        format!("Failed to write metadata to temp file: {:?}", temp_path)
    })?;
    file.sync_all().with_context(|| {
        format!("Failed to sync temp file: {:?}", temp_path)
    })?;

    std::fs::rename(&temp_path, &path).with_context(|| {
        format!("Failed to rename temp file to: {:?}", path)
    })?;

    Ok(())
}

/// Read `metadata.json` from `{root}/{user_id}/{session_id}/`.
pub(crate) fn load_from(
    root: &Path,
    user_id: &str,
    session_id: &str,
) -> Result<SessionMetadata> {
    let path = super::dir::session_path(root, user_id, session_id)
        .join("metadata.json");
    let content = fs::read_to_string(&path).with_context(|| {
        format!(
            "Failed to read metadata for session {:?} under user {:?}",
            session_id, user_id
        )
    })?;
    let metadata: SessionMetadata = serde_json::from_str(&content)?;
    // Enforce the on-disk invariant: metadata.owner_user_id MUST
    // match the directory's user_id. A mismatch signals either
    // corruption or a malicious cross-user move.
    if metadata.owner_user_id != user_id {
        return Err(StoreError::CrossUserAccess {
            caller: user_id.to_string(),
            owner: metadata.owner_user_id.clone(),
        }
        .into());
    }
    Ok(metadata)
}

/// RFC-3339 UTC timestamp for serialisation.
pub(crate) fn format_timestamp_utc(dt: &DateTime<Utc>) -> String {
    dt.to_rfc3339()
}
