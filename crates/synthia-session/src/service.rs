//! Concrete `Store` persistence API (formerly `PersistenceService` trait).
//!
//! This module historically decoupled higher-level session management from
//! any concrete storage implementation (disk, in-memory, remote, etc.) by
//! exposing a narrow async trait for the persistence operations used by
//! the session subsystem.
//!
//! The `PersistenceService` trait was REMOVED on 2026-06-15 in change
//! `2026-06-15-p2-trait-cleanup` because it had 0 trait-bound usage, 0 dyn
//! dispatch (the `AgentDependencies` field already used the concrete
//! `SessionStore` alias for `synthia_session::Store`), and exactly 1
//! real implementation (`Store`). Callers should use the synchronous
//! methods on `Store` directly (e.g. `Store::save_metadata`,
//! `Store::append_message`, `Store::load_messages_recent`, ...).
//!
//! The `metadata_to_session` helper remains for callers that previously
//! loaded a `Session` via the trait; pair it with
//! `Store::load_metadata` for the equivalent behaviour.

use anyhow::Result;

use crate::{
    error::StoreError,
    store::{SessionMetadata, Store},
    types::{Session, TokenBudget},
};

/// Convert persisted `SessionMetadata` back into an in-memory `Session`.
///
/// `SessionMetadata` is the on-disk projection of `Session`; fields that
/// are not persisted (token budget, context window, dirty flag) are
/// reset to their defaults. The `user_id` is reconstructed from
/// `owner_user_id`; callers MUST ensure `load_metadata` was issued
/// under the same `user_id` they pass here so that the on-disk
/// invariant is preserved.
pub fn metadata_to_session(metadata: SessionMetadata) -> Result<Session> {
    // The metadata came from a directory whose user_id was already
    // verified by `Store::load_metadata`. We still need to make sure
    // the in-memory session's `user_id` matches, so a downstream
    // `save_metadata` round-trip keeps the directory's user_id.
    if metadata.owner_user_id.is_empty() {
        return Err(StoreError::MissingUserId {
            session_id: metadata.id,
        }
        .into());
    }
    Ok(Session {
        id: metadata.id,
        user_id: metadata.owner_user_id,
        state: metadata.state,
        token_usage: metadata.token_usage,
        created_at: metadata
            .created_at
            .parse()
            .unwrap_or_else(|_| chrono::Utc::now()),
        updated_at: metadata
            .updated_at
            .parse()
            .unwrap_or_else(|_| chrono::Utc::now()),
        config: metadata.config,
        needs_save: false,
        token_budget: TokenBudget::default(),
        context_window: 128_000,
        end_reason: metadata.end_reason,
        iteration: metadata.iteration,
        cumulative_tokens: metadata.cumulative_tokens,
        context_token_limit: metadata.context_token_limit,
        parent_id: metadata.parent_id,
    })
}

/// Load a session by id under the given `user_id`, returning `None` if
/// it does not exist.
///
/// Convenience wrapper that combines `Store::session_exists`,
/// `Store::load_metadata`, and [`metadata_to_session`]. Replaces the
/// `PersistenceService::load_session` method that was removed in
/// `2026-06-15-p2-trait-cleanup`. The `user_id` parameter is
/// required so the on-disk path is namespaced.
pub fn load_session(
    store: &Store,
    user_id: &str,
    id: &str,
) -> Result<Option<Session>> {
    if !store.session_exists(user_id, id) {
        return Ok(None);
    }
    let metadata = store.load_metadata(user_id, id)?;
    Ok(Some(metadata_to_session(metadata)?))
}

#[cfg(test)]
mod tests {
    use synthia_provider::Message;
    use tempfile::TempDir;

    use super::*;
    use crate::store::CheckpointData;

    const TEST_USER: &str = "alice";

    fn make_store() -> (Store, TempDir) {
        let temp = TempDir::new().unwrap();
        (Store::new(temp.path().to_path_buf()), temp)
    }

    fn make_session(id: &str) -> Session {
        Session::new_with_user(id.to_string(), TEST_USER.to_string()).unwrap()
    }

    #[test]
    fn save_and_load_session_roundtrip() {
        let (store, _temp) = make_store();
        let session = make_session("svc-test");

        store.save_metadata(&session).unwrap();
        let loaded = load_session(&store, TEST_USER, "svc-test").unwrap();

        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.id, "svc-test");
        assert_eq!(loaded.user_id, TEST_USER);
        assert_eq!(loaded.state, session.state);
    }

    #[test]
    fn load_missing_session_returns_none() {
        let (store, _temp) = make_store();
        let loaded = load_session(&store, TEST_USER, "does-not-exist").unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn append_and_load_messages() {
        let (store, _temp) = make_store();

        store
            .append_message(TEST_USER, "s1", &Message::user("Hello"))
            .unwrap();
        store
            .append_message(TEST_USER, "s1", &Message::assistant("Hi there"))
            .unwrap();

        let recent: Vec<Message> =
            store.load_messages_recent(TEST_USER, "s1", 10).unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(
            recent[1].content.extract_text().as_deref(),
            Some("Hi there")
        );

        let all: Vec<Message> =
            store.load_messages_all(TEST_USER, "s1").unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn checkpoint_save_and_load() {
        let (store, _temp) = make_store();

        let checkpoint = CheckpointData {
            session_id: "s1".to_string(),
            step: 3,
            iteration: 1,
            messages: vec![Message::user("checkpointed")],
        };

        store.save_checkpoint(TEST_USER, "s1", &checkpoint).unwrap();
        let loaded = store.load_latest_checkpoint(TEST_USER, "s1").unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.step, 3);
        assert_eq!(loaded.session_id, "s1");
        assert_eq!(loaded.messages.len(), 1);
    }

    #[test]
    fn load_checkpoint_missing_returns_none() {
        let (store, _temp) = make_store();
        let loaded = store.load_latest_checkpoint(TEST_USER, "nope").unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn load_messages_recent_typed() {
        let (store, _temp) = make_store();

        store
            .append_message(TEST_USER, "s2", &Message::user("first"))
            .unwrap();
        store
            .append_message(TEST_USER, "s2", &Message::user("second"))
            .unwrap();

        let messages: Vec<Message> =
            store.load_messages_recent(TEST_USER, "s2", 1).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(
            messages[0].content.extract_text().as_deref(),
            Some("second")
        );
    }

    #[test]
    fn metadata_to_session_rejects_empty_owner() {
        // A metadata blob whose owner_user_id is empty must not be
        // turned into a session, because the in-memory session needs a
        // non-empty user_id.
        let meta = SessionMetadata {
            version: 1,
            id: "orphan".to_string(),
            owner_user_id: String::new(),
            state: crate::types::SessionState::Initializing,
            token_usage: Default::default(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            config: Default::default(),
            message_count: 0,
            end_reason: None,
            iteration: 0,
            cumulative_tokens: 0,
            context_token_limit: None,
            title: None,
            controller_version: 1,
            parent_id: None,
        };
        let err = metadata_to_session(meta).unwrap_err();
        let msg = format!("{:#}", err);
        assert!(
            msg.contains("MissingUserId") || msg.contains("user_id"),
            "got: {msg}"
        );
    }
}
