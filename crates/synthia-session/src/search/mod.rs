//! Session search — find an earlier conversation by its text.
//!
//! pi ships this as `SessionSearchService`
//! (`packages/agent/src/search/index.ts`): search the agent's *own*
//! history, not the model's knowledge. That is a product capability a
//! durable-transcript store earns the right to provide, and it was the one
//! capability the R62 reference-parity audit found missing here.
//!
//! Two pieces, in the order a consumer meets them:
//!
//! - [`SessionSearch`] — the seam. Four methods, runtime-neutral: a
//!   consumer can index in `SQLite`, in memory, or in a service.
//! - [`JsonlSessionSearch`] — the shipped implementation over the JSONL
//!   layout this crate already writes
//!   (`<root>/<user>/<session>/events.jsonl`). It folds each log through
//!   [`fold_log_surface`](crate::fold_log_surface) — the same function
//!   that reconstructs the model-visible surface — so it searches *what
//!   the model saw*, including compaction replacements, rather than raw
//!   log rows.
//!
//! ```text
//! <root>/alice/s_01/events.jsonl ─┐
//! <root>/alice/s_02/events.jsonl ─┼─ sync() ──▶ index ──▶ search_sessions()
//! <root>/bob/s_03/events.jsonl   ─┘                       search_entries()
//! ```
//!
//! The index is in memory and rebuilt on demand: a session log is
//! append-only, so `sync` only re-reads a file whose size changed, and a
//! deployment that never calls [`SessionSearch::sync`] pays nothing.
//!
//! # Tenancy
//!
//! The store holds every user's sessions side by side, and two users may
//! pick the same session id, so a session is addressed by the pair
//! `(user_id, session_id)` — the index key [`JsonlSessionSearch`] uses.
//! [`SearchQuery::user_id`] scopes a query: a scoped query reads only that
//! user's directory and answers only with that user's hits. Leaving it
//! `None` searches the whole store, which is deliberate for a caller that
//! owns all of it (an operator's admin index) and wrong for a request
//! handler — those resolve the caller and set the scope.
//!
//! # Scoring
//!
//! Deliberately simple and documented rather than tunable: an entry
//! scores `matched_terms / terms × 1 / (1 + ln(1 + terms_in_entry))` — term
//! coverage with a length penalty — and a session scores the sum of its
//! entry scores. That ranks "the conversation that talks about this a lot,
//! briefly" above a long transcript that mentions the word once, which is
//! the behaviour a user expects from a history search. A
//! deployment that wants BM25-style retrieval over session history
//! can implement [`SessionSearch`] and keep its own index.
//!
//! ## Module layout
//!
//! | Module | Responsibility |
//! |---|---|
//! | `query` | [`SearchQuery`] / [`EntryHit`] / [`SessionHit`] / [`SearchError`] + the [`SessionSearch`] seam. |
//! | `backend` | [`JsonlSessionSearch`] — the in-memory index over the JSONL store (`scan` / `index` / `sync` / `remove`). |
//! | `text` | The searchable text of a folded surface message. |
//! | `score` | Term coverage scoring, whole-word matching, and snippets. |

use std::{path::Path, time::SystemTime};

mod backend;
mod query;
mod score;
mod text;

pub use backend::{JsonlSessionSearch, jsonl_session_search};
pub use query::{
    DEFAULT_LIMIT,
    EntryHit,
    SearchError,
    SearchQuery,
    SessionHit,
    SessionSearch,
    SharedSessionSearch,
};

/// Modification time of a path, for callers that want to show "last
/// activity" next to a hit. `None` when the metadata is unavailable.
#[must_use]
pub fn modified_at(path: &Path) -> Option<SystemTime> {
    path.metadata().ok().and_then(|m| m.modified().ok())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    /// A log row shaped exactly like the sink writes them: the payload is
    /// a real `synthia_provider::Message` serialized, so the fixture cannot
    /// drift from the wire shape the fold expects.
    fn row(seq: u64, ts: &str, role: &str, text: &str) -> String {
        use synthia_provider::{Message, Role};
        let kind = if role == "user" {
            "user_message"
        } else {
            "assistant_message"
        };
        let message = if role == "user" {
            Message::user(text)
        } else {
            Message::new(Role::Assistant, synthia_provider::Content::text(text))
        };
        serde_json::json!({
            "type": kind,
            "seq": seq,
            "ts": ts,
            "data": message,
        })
        .to_string()
    }

    fn write_session(root: &Path, user: &str, session: &str, rows: &[String]) {
        let dir = root.join(user).join(session);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("events.jsonl"), rows.join("\n") + "\n")
            .unwrap();
    }

    /// Two sessions with distinguishable content, plus one whose only
    /// mention of the query is inside a tool *result*.
    fn store() -> (tempfile::TempDir, JsonlSessionSearch) {
        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path().join("sessions");
        write_session(
            &root,
            "alice",
            "s_auth",
            &[
                row(1, "2026-09-12T09:00:00Z", "user", "how do tokens expire"),
                row(
                    2,
                    "2026-09-12T09:00:02Z",
                    "assistant",
                    "Access tokens live 900 seconds; refresh tokens rotate.",
                ),
            ],
        );
        write_session(
            &root,
            "bob",
            "s_unrelated",
            &[row(
                1,
                "2026-09-12T10:00:00Z",
                "user",
                "rename the config file",
            )],
        );
        write_session(
            &root,
            "bob",
            "s_tool_noise",
            &[row(1, "2026-09-12T11:00:00Z", "user", "read the readme")],
        );
        (temp, JsonlSessionSearch::new(root))
    }

    #[tokio::test]
    async fn sync_indexes_every_session_and_is_idempotent() {
        let (_temp, search) = store();
        assert_eq!(
            search.indexed_sessions(),
            0,
            "nothing is indexed before sync"
        );
        assert_eq!(search.sync().await.unwrap(), 3);
        assert_eq!(search.sync().await.unwrap(), 3, "sync is idempotent");
    }

    #[tokio::test]
    async fn a_missing_store_syncs_to_empty_rather_than_failing() {
        let temp = tempfile::TempDir::new().unwrap();
        let search = JsonlSessionSearch::new(temp.path().join("nope"));
        assert_eq!(search.sync().await.unwrap(), 0);
        assert!(
            search
                .search_entries(&SearchQuery::new("anything"))
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn entries_are_found_with_a_snippet_and_their_seq() {
        let (_temp, search) = store();
        let hits = search
            .search_entries(&SearchQuery::new("refresh"))
            .await
            .unwrap();
        assert_eq!(hits.len(), 1, "one entry mentions it: {hits:?}");
        assert_eq!(hits[0].session_id, "s_auth");
        assert_eq!(hits[0].seq, 2, "the assistant row's seq");
        assert_eq!(hits[0].timestamp.as_deref(), Some("2026-09-12T09:00:02Z"));
        assert!(
            hits[0].snippet.contains("refresh"),
            "the snippet must show the match: {}",
            hits[0].snippet
        );
    }

    #[tokio::test]
    async fn matching_is_whole_word_and_case_insensitive() {
        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path().join("sessions");
        write_session(
            &root,
            "u",
            "s1",
            &[row(
                1,
                "2026-09-12T09:00:00Z",
                "user",
                "I already READ the README",
            )],
        );
        let search = JsonlSessionSearch::new(&root);
        assert!(
            search
                .search_entries(&SearchQuery::new("read"))
                .await
                .unwrap()
                .len()
                == 1,
            "case-insensitive whole-word match"
        );
        assert!(
            search
                .search_entries(&SearchQuery::new("readme"))
                .await
                .unwrap()
                .len()
                == 1
        );
        // "read" inside "already" is not a match on its own.
        let temp2 = tempfile::TempDir::new().unwrap();
        let root2 = temp2.path().join("sessions");
        write_session(
            &root2,
            "u",
            "s1",
            &[row(1, "2026-09-12T09:00:00Z", "user", "already done")],
        );
        let search2 = JsonlSessionSearch::new(&root2);
        assert!(
            search2
                .search_entries(&SearchQuery::new("read"))
                .await
                .unwrap()
                .is_empty(),
            "a substring inside a word must not match"
        );
    }

    #[tokio::test]
    async fn sessions_rank_by_relevance_and_report_their_match_count() {
        let (_temp, search) = store();
        let hits = search
            .search_sessions(&SearchQuery::new("tokens"))
            .await
            .unwrap();
        assert_eq!(hits.len(), 1, "only one session talks about tokens");
        assert_eq!(hits[0].session_id, "s_auth");
        assert_eq!(hits[0].matched_entries, 2, "both rows mention it");
        assert!(
            hits[0].top.is_some(),
            "the summary line carries the best row"
        );
    }

    #[tokio::test]
    async fn limit_and_session_filter_apply() {
        let (_temp, search) = store();
        let all = search
            .search_entries(&SearchQuery::new("the"))
            .await
            .unwrap();
        assert!(all.len() > 1, "several entries mention 'the': {all:?}");
        let capped = search
            .search_entries(&SearchQuery::new("the").with_limit(1))
            .await
            .unwrap();
        assert_eq!(capped.len(), 1);

        let filtered = search
            .search_entries(&SearchQuery::new("the").in_session("s_unrelated"))
            .await
            .unwrap();
        assert!(
            filtered.iter().all(|hit| hit.session_id == "s_unrelated"),
            "{filtered:?}"
        );
    }

    #[tokio::test]
    async fn a_removed_session_is_no_longer_searchable() {
        let (_temp, search) = store();
        search.sync().await.unwrap();
        assert!(search.remove("alice", "s_auth").await.unwrap());
        assert!(
            !search.remove("alice", "s_auth").await.unwrap(),
            "removal is idempotent"
        );
        assert!(
            search
                .search_entries(&SearchQuery::new("tokens"))
                .await
                .unwrap()
                .is_empty(),
            "the removed session must be gone"
        );
        // The log is still on disk — the tombstone is what keeps it out.
        assert_eq!(
            search.sync().await.unwrap(),
            2,
            "the tombstone survives sync"
        );
        assert!(
            search
                .search_entries(&SearchQuery::new("tokens"))
                .await
                .unwrap()
                .is_empty(),
            "a sync must not resurrect a forgotten session"
        );
    }

    #[tokio::test]
    async fn an_appended_row_is_picked_up_by_the_next_sync() {
        let (temp, search) = store();
        assert_eq!(search.sync().await.unwrap(), 3);
        assert!(
            search
                .search_entries(&SearchQuery::new("webhook"))
                .await
                .unwrap()
                .is_empty()
        );
        let path = temp
            .path()
            .join("sessions")
            .join("alice")
            .join("s_auth")
            .join("events.jsonl");
        let mut raw = std::fs::read_to_string(&path).unwrap();
        raw.push_str(&row(
            3,
            "2026-09-12T09:01:00Z",
            "user",
            "and a webhook refreshes it",
        ));
        raw.push('\n');
        std::fs::write(&path, raw).unwrap();

        let hits = search
            .search_entries(&SearchQuery::new("webhook"))
            .await
            .unwrap();
        assert_eq!(hits.len(), 1, "the new row must be indexed: {hits:?}");
        assert_eq!(hits[0].seq, 3);
    }

    /// Two tenants whose transcripts mention different words — and who
    /// deliberately picked the *same* session id, which is the case the
    /// index has to keep apart.
    fn tenant_store() -> (tempfile::TempDir, JsonlSessionSearch) {
        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path().join("sessions");
        write_session(
            &root,
            "alice",
            "main",
            &[row(
                1,
                "2026-09-12T09:00:00Z",
                "user",
                "my secret is the nebula passphrase",
            )],
        );
        write_session(
            &root,
            "bob",
            "main",
            &[row(
                1,
                "2026-09-12T10:00:00Z",
                "user",
                "my secret is the quasar passphrase",
            )],
        );
        (temp, JsonlSessionSearch::new(root))
    }

    /// The disclosure this index must never repeat: a query scoped to one
    /// user may not be answered with another user's transcript — not as a
    /// lower-ranked hit, not as a snippet, not at all. The two users share
    /// a session id, so an index keyed by that id alone would answer
    /// "alice" from bob's log even when the directory walk is correct.
    #[tokio::test]
    async fn a_scoped_search_never_answers_with_another_users_session() {
        let (_temp, search) = tenant_store();
        assert_eq!(search.sync().await.unwrap(), 2, "one session per tenant");

        // A term only bob's log carries: scoping to alice must find
        // nothing rather than reach across tenants.
        let leaked = search
            .search_entries(&SearchQuery::new("quasar").in_user("alice"))
            .await
            .unwrap();
        assert!(
            leaked.is_empty(),
            "alice's search was answered with bob's transcript: {leaked:?}"
        );

        // The shared term is carried by both logs; the scoped answer must
        // be alice's row only — neither a second hit nor a snippet from
        // bob's.
        let hits = search
            .search_entries(&SearchQuery::new("secret").in_user("alice"))
            .await
            .unwrap();
        assert_eq!(hits.len(), 1, "only alice's entry may match: {hits:?}");
        assert!(
            hits[0].snippet.contains("nebula")
                && !hits[0].snippet.contains("quasar"),
            "the snippet must come from alice's log: {}",
            hits[0].snippet
        );

        // Same guarantee one level up, where the leak was reported: the
        // session hit — id included — belongs to the scoped user.
        let sessions = search
            .search_sessions(&SearchQuery::new("secret").in_user("alice"))
            .await
            .unwrap();
        assert_eq!(sessions.len(), 1, "{sessions:?}");
        assert_eq!(sessions[0].session_id, "main");
        assert_eq!(sessions[0].matched_entries, 1);

        // Bob's side of the same store, for symmetry: his scope finds his
        // own transcript and not alice's.
        let bob = search
            .search_entries(&SearchQuery::new("secret").in_user("bob"))
            .await
            .unwrap();
        assert_eq!(bob.len(), 1, "{bob:?}");
        assert!(
            bob[0].snippet.contains("quasar")
                && !bob[0].snippet.contains("nebula"),
            "the snippet must come from bob's log: {}",
            bob[0].snippet
        );
        assert!(
            search
                .search_entries(
                    &SearchQuery::new("nebula")
                        .in_user("bob")
                        .in_session("main")
                )
                .await
                .unwrap()
                .is_empty(),
            "a session id is scoped to the user who owns it"
        );
    }

    /// The unscoped meaning stays deliberate: `user_id: None` is "the
    /// whole store", which is what a caller that owns every tenant (an
    /// operator's admin index) asks for. Two users holding the same
    /// session id both answer — proof the index keeps them apart instead
    /// of folding one onto the other.
    #[tokio::test]
    async fn an_unscoped_search_still_spans_every_user() {
        let (_temp, search) = tenant_store();

        let hits = search
            .search_entries(&SearchQuery::new("quasar"))
            .await
            .unwrap();
        assert_eq!(hits.len(), 1, "bob's row is reachable unscoped: {hits:?}");
        assert_eq!(hits[0].session_id, "main");

        let sessions = search
            .search_sessions(&SearchQuery::new("secret"))
            .await
            .unwrap();
        assert_eq!(
            sessions.len(),
            2,
            "both same-id sessions must be indexed: {sessions:?}"
        );
        assert!(
            sessions.iter().all(|hit| hit.session_id == "main"),
            "{sessions:?}"
        );
    }

    /// Forget is addressed by user *and* session: forgetting alice's
    /// `main` may not hide bob's.
    #[tokio::test]
    async fn forgetting_one_users_session_leaves_the_other_users_alone() {
        let (_temp, search) = tenant_store();
        search.sync().await.unwrap();

        assert!(search.remove("alice", "main").await.unwrap());
        assert!(
            search
                .search_entries(&SearchQuery::new("nebula").in_user("alice"))
                .await
                .unwrap()
                .is_empty(),
            "the forgotten session is unsearchable"
        );
        assert_eq!(
            search
                .search_entries(&SearchQuery::new("quasar").in_user("bob"))
                .await
                .unwrap()
                .len(),
            1,
            "bob's same-named session is untouched"
        );
        assert_eq!(
            search.sync().await.unwrap(),
            1,
            "only bob's session is still indexed"
        );
    }

    /// A scoped search must not *read* another user's logs — filtering
    /// after reading everyone's is the shape of the reported bug. The other
    /// tenant's log is poisoned into something unreadable as a file, so the
    /// difference is observable: the unscoped scan trips over it while the
    /// scoped one never opens it.
    #[tokio::test]
    async fn a_scoped_scan_never_reads_another_users_log() {
        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path().join("sessions");
        write_session(
            &root,
            "alice",
            "main",
            &[row(1, "2026-09-12T09:00:00Z", "user", "alice nebula")],
        );
        // A directory where a log should be: readable metadata, unreadable
        // contents.
        std::fs::create_dir_all(
            root.join("bob").join("main").join("events.jsonl"),
        )
        .unwrap();
        let search = JsonlSessionSearch::new(&root);

        let hits = search
            .search_entries(&SearchQuery::new("nebula").in_user("alice"))
            .await
            .expect("alice's search must not touch bob's log");
        assert_eq!(hits.len(), 1, "{hits:?}");

        assert!(
            search.sync().await.is_err(),
            "the unscoped scan is the one that reaches that log, so \
             alice's success above is not vacuous"
        );
    }

    #[tokio::test]
    async fn an_empty_query_matches_nothing() {
        let (_temp, search) = store();
        assert!(
            search
                .search_sessions(&SearchQuery::default())
                .await
                .unwrap()
                .is_empty()
        );
    }
}
