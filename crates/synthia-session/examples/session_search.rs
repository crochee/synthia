//! Session search: find an earlier conversation by its text.
//!
//! Seam shown: [`synthia_session::SessionSearch`] — the trait pi ships as
//! `SessionSearchService` — and its shipped implementation over the JSONL
//! layout this crate writes (`<root>/<user>/<session>/events.jsonl`). The
//! index folds each log through `fold_log_surface`, so a query matches
//! *what the model saw* (including compaction replacements) rather than
//! raw log rows.
//!
//! Run:
//!
//! ```text
//! cargo run -p synthia-session --example session_search
//! ```
//!
//! Look at:
//!
//! 1. `sync()` — the index is built from disk and rebuilt only for logs
//!    whose size changed, so an already-indexed store costs a `stat` per
//!    session;
//! 2. `search_sessions("token refresh")` — term coverage ranks the session
//!    that uses both terms (`0.303`) above one that uses a single one
//!    (`0.156`), and the hit carries the best entry's *seq*, timestamp and
//!    snippet so a UI can jump straight to it. Matching is whole-word and
//!    case-insensitive, so `tokens` does not answer a query for `token`;
//! 3. `remove()` — a forgotten session stays unsearchable across a later
//!    `sync()`, which matters when the log is deleted after the index was
//!    built.

use std::sync::Arc;

use synthia_provider::{Content, Message, Role};
use synthia_session::{JsonlSessionSearch, SearchQuery, SessionSearch};

/// One log row, shaped exactly like `JsonlSessionSink` writes it.
fn row(seq: u64, ts: &str, text: &str, user: bool) -> String {
    let message = if user {
        Message::user(text)
    } else {
        Message::new(Role::Assistant, Content::text(text))
    };
    serde_json::json!({
        "type": if user { "user_message" } else { "assistant_message" },
        "seq": seq,
        "ts": ts,
        "data": message,
    })
    .to_string()
}

fn write_session(
    root: &std::path::Path,
    user: &str,
    session: &str,
    rows: &[String],
) {
    let dir = root.join(user).join(session);
    std::fs::create_dir_all(&dir).expect("create session dir");
    std::fs::write(dir.join("events.jsonl"), rows.join("\n") + "\n")
        .expect("write log");
}

#[tokio::main]
async fn main() {
    let root = std::env::temp_dir()
        .join(format!("synthia-session-search-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let sessions = root.join("sessions");

    // Two conversations, one of which is *about* the query.
    write_session(
        &sessions,
        "alice",
        "s_tokens",
        &[
            row(
                1,
                "2026-09-12T09:00:00Z",
                "how do access tokens expire",
                true,
            ),
            row(
                2,
                "2026-09-12T09:00:02Z",
                "Access tokens live 900 seconds. A refresh token rotates it.",
                false,
            ),
        ],
    );
    write_session(
        &sessions,
        "bob",
        "s_config",
        &[
            row(1, "2026-09-12T10:00:00Z", "rename the config file", true),
            row(
                2,
                "2026-09-12T10:00:03Z",
                "Done — the token in it was renamed too.",
                false,
            ),
        ],
    );

    let search = JsonlSessionSearch::new(&sessions);
    println!(
        "=== synthia: session search (pi SessionSearchService parity) ===\n"
    );
    println!("store            : {}", sessions.display());
    println!("indexed (before) : {}", search.indexed_sessions());

    let indexed = search.sync().await.expect("sync");
    println!("indexed (sync)   : {indexed} sessions");
    println!(
        "indexed (again)  : {} sessions (only changed logs are re-read)",
        search.sync().await.expect("sync")
    );

    // Two-term query: term coverage decides the order.
    let hits = search
        .search_sessions(&SearchQuery::new("token refresh"))
        .await
        .expect("search");
    println!("\nquery            : \"token refresh\"");
    for hit in &hits {
        println!(
            "  {}  score={:.3}  entries={}",
            hit.session_id, hit.score, hit.matched_entries
        );
        if let Some(top) = &hit.top {
            println!(
                "      seq {} at {}: {}",
                top.seq,
                top.timestamp.as_deref().unwrap_or("-"),
                top.snippet
            );
        }
    }

    // One-term query: both sessions mention "token", so both are hits.
    let broad = search
        .search_entries(&SearchQuery::new("token"))
        .await
        .expect("search");
    println!("\nquery            : \"token\" (entries)");
    for hit in &broad {
        println!(
            "  {}#{}  score={:.3}  {}",
            hit.session_id, hit.seq, hit.score, hit.snippet
        );
    }

    // Forget a session: it stays unsearchable even after a rescan.
    let forgotten = search.remove("bob", "s_config").await.expect("remove");
    let after = search
        .search_entries(&SearchQuery::new("token"))
        .await
        .expect("search");
    println!(
        "\nremove(s_config) : indexed={forgotten}, remaining hits={}",
        after.len()
    );

    assert_eq!(indexed, 2, "both sessions are indexed");
    assert_eq!(
        hits.first().map(|hit| hit.session_id.as_str()),
        Some("s_tokens"),
        "the session that talks about both terms must rank first"
    );
    assert_eq!(
        hits.first().map(|hit| hit.matched_entries),
        Some(1),
        "only the assistant row matches both terms (\"tokens\" is not \"token\")"
    );
    assert!(
        hits[0].top.as_ref().is_some_and(|top| top.seq == 2),
        "the assistant row is the best entry"
    );
    assert_eq!(broad.len(), 2, "both sessions mention 'token'");
    assert!(forgotten, "s_config was indexed before removal");
    assert_eq!(after.len(), 1, "the forgotten session stays gone");
    assert_eq!(after[0].session_id, "s_tokens");

    // Same seam, any backend: the shared handle is what a server stores.
    let shared: synthia_session::SharedSessionSearch =
        Arc::new(JsonlSessionSearch::new(&sessions));
    assert_eq!(shared.sync().await.expect("sync"), 2);

    let _ = std::fs::remove_dir_all(&root);
    println!("\nSESSION-SEARCH: OK");
}
