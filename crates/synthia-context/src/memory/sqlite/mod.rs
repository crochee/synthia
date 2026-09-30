//! SQLite-backed memory tiers (traitclaw-memory-sqlite parity).
//!
//! [`SqliteMemory`] is the *single-file* durable backend: one
//! `SQLite` database holding long-term entries, working memory, and
//! the session registry. Where [`FileMemory`](super::FileMemory)
//! trades recall quality for a human-readable, git-shareable
//! directory tree, this backend trades readability for an index:
//! long-term recall is an `FTS5` (BM25) query rather than a keyword
//! scan, and a crash cannot leave a half-written entry behind,
//! because a write is one transaction.
//!
//! # What lives where
//!
//! | Tier | Backing | Why |
//! |---|---|---|
//! | Conversation | inner [`Memory`] (session sink) | the durable event log is already the source of truth; a second copy could only diverge |
//! | Working | `working_memory` table | a resumed run should find its scratch state, not an empty map |
//! | Long-term | `long_term_memory` + `FTS5` | semantic recall across sessions |
//! | Session registry | `sessions` table | list/delete need a directory this backend does not have |
//!
//! The conversation tier therefore delegates, exactly like
//! [`FileMemory`](super::FileMemory) — a deliberate divergence from
//! traitclaw's `SqliteMemory`, which also stores messages: synthia's
//! conversation tier already has a durable sink, and duplicating it
//! would make two sources of truth for the same history.
//!
//! # Recall
//!
//! A query is sanitised into `FTS5` terms (alphanumeric runs, each
//! quoted, joined with `OR`) so punctuation and `FTS5` operators can
//! never be read as syntax — `"NEAR("` is a term, not a syntax
//! error. Results are ordered by BM25 (best first), then newest
//! first, so equal-scoring hits are deterministic. A query with no
//! usable terms (empty, or only punctuation) returns the newest
//! entries, matching the trait's "an empty query matches all
//! entries".
//!
//! # Errors
//!
//! [`SqliteMemoryError`] carries a reason string rather than the
//! `rusqlite::Error` itself: [`MemoryError`] is `Clone + Eq`, and
//! `rusqlite::Error` is neither. The message keeps `SQLite`'s own
//! text (`no such table`, `database is locked`), which is what a log
//! consumer acts on.
//!
//! ## Module layout
//!
//! | Module | Responsibility |
//! |---|---|
//! | `schema` | The idempotent DDL applied on first open + the schema probe. |
//! | `error` | [`SqliteMemoryError`] + the query-error helper. |
//! | `store` | [`SqliteMemory`] — constructors, pragmas, recall, and entry writes. |
//! | `decode` | Row decoding (raw columns → `MemoryEntry`) and the sortable timestamp form. |
//! | `fts` | `FTS5` query sanitising — every user term quoted, so operators stay literal. |
//! | `memory_impl` | The [`Memory`](crate::memory::Memory) trait impl (working sessions + delegation). |

mod decode;
mod error;
mod fts;
mod memory_impl;
mod schema;
mod store;

pub use error::SqliteMemoryError;
pub use store::SqliteMemory;

#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use serde_json::json;
    use synthia_provider::Message;
    use ulid::Ulid;

    use super::*;
    use crate::memory::{Memory, MemoryEntry, MemoryError};

    fn memory() -> SqliteMemory {
        SqliteMemory::in_memory().expect("in-memory database")
    }

    fn entry(id: &str, content: &str) -> MemoryEntry {
        MemoryEntry::now(id, content)
    }

    #[test]
    fn search_ranks_by_relevance_then_recency() {
        let mem = memory();
        mem.store_entry(&entry("01ARZ3NDEKTSV4RRFFQ69G5FAV", "alpha beta"))
            .expect("store");
        mem.store_entry(&entry("01ARZ3NDEKTSV4RRFFQ69G5FAW", "alpha only"))
            .expect("store");
        mem.store_entry(&entry("01ARZ3NDEKTSV4RRFFQ69G5FAX", "gamma"))
            .expect("store");

        let hits = mem.search("alpha beta", 10).expect("search");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].content, "alpha beta");

        // A query with no terms returns the newest entries.
        let all = mem.search("", 10).expect("search all");
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].id, "01ARZ3NDEKTSV4RRFFQ69G5FAX");

        assert_eq!(mem.search("alpha", 1).expect("limited").len(), 1);
    }

    #[test]
    fn fts_operators_in_a_query_are_terms_not_syntax() {
        let mem = memory();
        mem.store_entry(&entry("01ARZ3NDEKTSV4RRFFQ69G5FAV", "a NEAR( b"))
            .expect("store");

        // A raw `NEAR(` would be an FTS5 syntax error; it must be
        // searched as literal text instead.
        let hits = mem.search("NEAR(", 5).expect("search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].content, "a NEAR( b");
        // A query with no searchable terms behaves like the empty
        // query — match all — rather than raising an FTS5 error.
        assert_eq!(mem.search("*", 5).expect("search").len(), 1);
        assert_eq!(mem.search("   ", 5).expect("search").len(), 1);
        // A term that matches nothing still returns nothing.
        assert!(mem.search("zzz", 5).expect("search").is_empty());
    }

    #[test]
    fn storing_an_id_twice_replaces_it_in_the_index() {
        let mem = memory();
        let id = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
        mem.store_entry(&entry(id, "before")).expect("store");
        mem.store_entry(&entry(id, "after")).expect("replace");

        assert!(mem.search("before", 5).expect("search").is_empty());
        let hits = mem.search("after", 5).expect("search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].content, "after");
        assert_eq!(mem.search("", 10).expect("all").len(), 1);

        // A non-ULID id is replaced by a fresh one (file-parity).
        let mut anonymous = MemoryEntry::now("", "generated id");
        anonymous.metadata = Some(json!({"kind": "note"}));
        mem.store_entry(&anonymous).expect("store anonymous");
        let hits = mem.search("generated", 5).expect("search");
        assert_eq!(hits[0].metadata, Some(json!({"kind": "note"})));
        assert!(Ulid::from_string(&hits[0].id).is_ok());
    }

    #[tokio::test]
    async fn working_memory_and_sessions_survive_a_reopen() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("memory.db");
        let session = {
            let mem = SqliteMemory::open(&path).expect("open");
            let session = mem.create_session().await.expect("session");
            mem.set_context(&session, "task", json!({"step": 2}))
                .await
                .expect("set");
            session
        };

        let reopened = SqliteMemory::open(&path).expect("reopen");
        assert_eq!(
            reopened.get_context(&session, "task").await.expect("get"),
            Some(json!({"step": 2}))
        );
        assert_eq!(
            reopened.list_sessions().await.expect("list"),
            vec![session.clone()]
        );

        reopened.delete_session(&session).await.expect("delete");
        assert_eq!(
            reopened.get_context(&session, "task").await.expect("get"),
            None
        );
        assert!(reopened.list_sessions().await.expect("list").is_empty());
    }

    #[tokio::test]
    async fn long_term_entries_survive_a_reopen() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("memory.db");
        {
            let mem = SqliteMemory::open(&path).expect("open");
            mem.store(entry("01ARZ3NDEKTSV4RRFFQ69G5FAV", "durable fact"))
                .await
                .expect("store");
        }
        let reopened = SqliteMemory::open(&path).expect("reopen");
        let hits = reopened.recall("durable", 5).await.expect("recall");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].content, "durable fact");
    }

    #[tokio::test]
    async fn conversation_tier_delegates_to_the_inner() {
        let mem = memory();
        let session = mem.create_session().await.expect("session");
        mem.append(&session, Message::user("hello"))
            .await
            .expect("append");
        assert_eq!(
            mem.messages(&session).await.expect("messages"),
            vec![Message::user("hello")]
        );
    }

    #[tokio::test]
    async fn read_only_refuses_writes_and_keeps_recall() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("memory.db");
        {
            let writable = SqliteMemory::open(&path).expect("open");
            writable
                .store(entry("01ARZ3NDEKTSV4RRFFQ69G5FAV", "read me"))
                .await
                .expect("store");
        }

        let reader = SqliteMemory::open_read_only(&path).expect("read-only");
        assert!(reader.is_read_only());
        assert_eq!(reader.recall("read", 5).await.expect("recall").len(), 1);
        assert_eq!(
            reader
                .store(entry("01ARZ3NDEKTSV4RRFFQ69G5FAW", "no"))
                .await,
            Err(MemoryError::Sqlite(SqliteMemoryError::ReadOnly))
        );
        assert_eq!(
            reader.create_session().await,
            Err(MemoryError::Sqlite(SqliteMemoryError::ReadOnly))
        );
        assert_eq!(
            reader.set_context("s", "k", json!(1)).await,
            Err(MemoryError::Sqlite(SqliteMemoryError::ReadOnly))
        );
    }

    #[test]
    fn read_only_open_requires_an_existing_schema() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("empty.db");
        Connection::open(&path).expect("create file");
        let error =
            SqliteMemory::open_read_only(&path).expect_err("schema is missing");
        assert!(
            matches!(error, SqliteMemoryError::MissingSchema { .. }),
            "{error}"
        );
    }
}
