//! `GET /api/v1/sessions` — the user's own session listing.
//!
//! The listing is a management view of the user's conversations, so
//! the throwaway sessions `POST /v1/messages` mints for clients that
//! name no session must not appear in it: a single Cursor or Claude
//! Code session filled the page with a dozen `anthropic-<uuid>` rows
//! in one verification pass. What these tests hold is the boundary of
//! that exclusion — the exact namespace form, the lookalike that is
//! *not* excluded, and the direct lookup that is deliberately still
//! served.

use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use synthia_server::{create_router, state::AppState};
use tower::ServiceExt;

/// The user this deployment resolves an unauthenticated request to:
/// the fixture must write into *that* namespace, or the test would be
/// asserting the leak instead of the fix.
const RESOLVED_USER: &str = synthia::session::manager::SERVER_DEFAULT_USER_ID;

/// A finished unbound Messages turn: on disk, evicted from the
/// registry when the turn ended.
const EPHEMERAL_ON_DISK: &str = "anthropic-9f1c4c6e8a2b4d7f";

/// An unbound Messages turn still open, i.e. registered rather than
/// durable — the other of the listing's two sources.
const EPHEMERAL_REGISTERED: &str = "anthropic-c4f7a0d21b8e4e69";

/// An id that shares the prefix's letters without its separator.
const LOOKALIKE: &str = "anthropicx-1";

/// The user's own conversation.
const CONVERSATION: &str = "my-conversation";

/// One transcript under `<root>/<user>/<session>/events.jsonl`.
fn write_transcript(root: &std::path::Path, session: &str, text: &str) {
    let dir = root.join(RESOLVED_USER).join(session);
    std::fs::create_dir_all(&dir).unwrap();
    let row = serde_json::json!({
        "type": "user_message",
        "seq": 1,
        "ts": "2026-09-19T09:00:00Z",
        "data": synthia::provider::Message::user(text),
    });
    std::fs::write(dir.join("events.jsonl"), format!("{row}\n")).unwrap();
}

/// The listing endpoints, over both of the sources they merge.
async fn app() -> (tempfile::TempDir, axum::Router) {
    let temp = tempfile::TempDir::new().unwrap();
    let sessions = temp.path().join("sessions");
    write_transcript(&sessions, EPHEMERAL_ON_DISK, "say hi");
    write_transcript(&sessions, LOOKALIKE, "a user's own session");
    write_transcript(&sessions, CONVERSATION, "how do access tokens expire");

    let registry = synthia::session::manager::SessionRegistry::new(sessions);
    let state = AppState::for_test(registry, temp.path().to_path_buf()).await;
    state
        .session_manager
        .create_with_user(
            EPHEMERAL_REGISTERED.to_string(),
            RESOLVED_USER.to_string(),
        )
        .await
        .expect("register a live minted session");
    state
        .session_manager
        .create_with_user(CONVERSATION.to_string(), RESOLVED_USER.to_string())
        .await
        .expect("register the user's session");
    (temp, create_router(Arc::new(state)).await)
}

async fn get(app: &axum::Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let resp = app
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json =
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

/// The ids the listing returned, in order.
fn listed_ids(body: &serde_json::Value) -> Vec<&str> {
    body["data"]
        .as_array()
        .expect("data array")
        .iter()
        .map(|row| row["id"].as_str().expect("id"))
        .collect()
}

/// The listing carries the user's conversations and nothing minted for
/// a client that named no session — from either source: one minted
/// session is durable (its turn finished and closed) and one is
/// registered (its turn is still open), so a filter covering only one
/// of the listing's two loops leaves a row behind and fails this test.
#[tokio::test]
async fn the_listing_omits_both_sources_of_unbound_messages_session() {
    let (_temp, app) = app().await;
    let (status, body) = get(&app, "/api/v1/sessions").await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let ids = listed_ids(&body);
    assert!(
        !ids.contains(&EPHEMERAL_ON_DISK),
        "a finished minted session must not be listed: {ids:?}"
    );
    assert!(
        !ids.contains(&EPHEMERAL_REGISTERED),
        "a live minted session must not be listed: {ids:?}"
    );
    assert!(ids.contains(&CONVERSATION), "listed: {ids:?}");
}

/// The exclusion is the namespace, not the letters: an id that merely
/// begins with `anthropic` is an ordinary session and stays listed.
/// This is the boundary the route implements (`anthropic-` — separator
/// included — not `anthropic`), asserted against the live endpoint so
/// a widened prefix cannot pass silently.
#[tokio::test]
async fn the_listing_keeps_an_id_that_only_shares_the_prefix_letters() {
    let (_temp, app) = app().await;
    let (status, body) = get(&app, "/api/v1/sessions").await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let ids = listed_ids(&body);
    assert!(ids.contains(&LOOKALIKE), "listed: {ids:?}");
}

/// Naming an ephemeral session is a deliberate lookup, not passive
/// browsing, so the detail route still serves it — the fix hides a row
/// from the sessions page, it does not make the session unreachable.
#[tokio::test]
async fn an_ephemeral_session_is_still_reachable_by_id() {
    let (_temp, app) = app().await;
    let (status, body) =
        get(&app, &format!("/api/v1/sessions/{EPHEMERAL_ON_DISK}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["id"].as_str(), Some(EPHEMERAL_ON_DISK));
    assert_eq!(body["history"].as_array().map(Vec::len), Some(1), "{body}");
}
