//! `GET /api/v1/sessions/search` — the HTTP face of the session-search
//! seam (R63).
//!
//! The search itself is covered by `synthia-session`'s unit tests; what
//! these tests prove is the *wiring*: the store the server writes is the
//! store it searches, the route is reachable (not shadowed by
//! `/sessions/{id}`), the envelope matches the v1 shape, and the search is
//! scoped to the user the request resolved to — a second tenant's
//! transcript, sharing a session id with the first, must not be reachable.

use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use synthia_server::{create_router, state::AppState};
use tower::ServiceExt;

/// One log row shaped exactly like the sink writes it.
fn row(seq: u64, ts: &str, text: &str, user: bool) -> String {
    use synthia::provider::{Content, Message, Role};
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

/// The user this deployment resolves an unauthenticated request to: the
/// fixture must write into *that* namespace, not a hardcoded one, or the
/// test would be asserting the leak instead of the fix.
const RESOLVED_USER: &str = synthia::session::manager::SERVER_DEFAULT_USER_ID;

/// Write one tenant's transcript at `<root>/<user>/<session>/events.jsonl`.
fn write_transcript(
    root: &std::path::Path,
    user: &str,
    session: &str,
    rows: &[String],
) {
    let dir = root.join(user).join(session);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("events.jsonl"), rows.join("\n") + "\n").unwrap();
}

/// The store: the resolved user's own session, plus a second tenant's —
/// deliberately under the **same session id**, which is the half of the
/// disclosure bug an index keyed by that id alone cannot survive.
async fn app_with_history() -> (tempfile::TempDir, axum::Router) {
    let temp = tempfile::TempDir::new().unwrap();
    let sessions = temp.path().join("sessions");
    write_transcript(
        &sessions,
        RESOLVED_USER,
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
                "They live 900 seconds and then refresh.",
                false,
            ),
        ],
    );
    write_transcript(
        &sessions,
        "other",
        "s_tokens",
        &[row(
            1,
            "2026-09-12T10:00:00Z",
            "the zephyr deployment key",
            true,
        )],
    );
    // A second session belonging to that tenant under a *distinct* id: the
    // same-id pair above is what the index must keep apart, and this one
    // makes "another tenant's transcript is unreachable" independent of
    // which of the two same-id logs won the scan.
    write_transcript(
        &sessions,
        "other",
        "s_runbook",
        &[row(1, "2026-09-12T10:05:00Z", "the zephyr runbook", true)],
    );

    let registry = synthia::session::manager::SessionRegistry::new(sessions);
    let state = AppState::for_test(registry, temp.path().to_path_buf()).await;
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

/// The route finds a session by its text, and the store it searches is
/// the one `SessionRegistry` writes into.
#[tokio::test]
async fn search_finds_a_session_by_its_text() {
    let (_temp, app) = app_with_history().await;
    let (status, body) = get(&app, "/api/v1/sessions/search?q=refresh").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let hits = body["data"].as_array().expect("data array");
    assert_eq!(hits.len(), 1, "{body}");
    assert_eq!(hits[0]["session_id"], "s_tokens");
    assert_eq!(hits[0]["matched_entries"], 1);
    assert_eq!(hits[0]["top"]["seq"], 2);
    assert!(
        hits[0]["top"]["snippet"]
            .as_str()
            .unwrap_or_default()
            .contains("refresh"),
        "{body}"
    );
    // The v1 envelope carries no cursor for a search (the search owns
    // ordering) and a `total` only when the limit was not reached.
    assert!(body.get("next_cursor").is_none(), "{body}");
    assert_eq!(body["total"], 1, "{body}");
}

/// The disclosure this route must never repeat: the search resolves the
/// requesting user, so another tenant's transcript — seeded here under the
/// same session id — is not a lower-ranked result, it is not a result.
#[tokio::test]
async fn a_tenant_cannot_search_another_users_history() {
    let (_temp, app) = app_with_history().await;

    // A term only the other tenant's transcript carries: both the hits and
    // the count must be empty, so nothing about that log is echoed back.
    let (status, body) = get(&app, "/api/v1/sessions/search?q=zephyr").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"].as_array().map(Vec::len),
        Some(0),
        "another tenant's transcript leaked: {body}"
    );
    assert_eq!(body["total"], 0, "{body}");

    // The same-session-id collision: the requesting user's own transcript
    // still answers, with their own snippet and not the other tenant's.
    let (status, body) = get(&app, "/api/v1/sessions/search?q=refresh").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let hits = body["data"].as_array().expect("data array");
    assert_eq!(hits.len(), 1, "{body}");
    assert_eq!(hits[0]["session_id"], "s_tokens");
    let snippet = hits[0]["top"]["snippet"].as_str().unwrap_or_default();
    assert!(snippet.contains("refresh"), "{body}");
    assert!(
        !snippet.contains("zephyr"),
        "the other tenant's transcript leaked into the snippet: {body}"
    );
}

/// A query with no matches is an empty successful response, not a 404 —
/// i.e. the literal `/sessions/search` route is not being captured by
/// `/sessions/{id}`.
#[tokio::test]
async fn a_query_with_no_matches_returns_an_empty_list() {
    let (_temp, app) = app_with_history().await;
    let (status, body) =
        get(&app, "/api/v1/sessions/search?q=kubernetes").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"].as_array().map(Vec::len), Some(0), "{body}");
    assert_eq!(body["total"], 0);

    // …and the capture route still resolves ids normally.
    let (status, _) = get(&app, "/api/v1/sessions/does-not-exist").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// An empty query is rejected by validation rather than matching
/// everything.
#[tokio::test]
async fn an_empty_query_is_rejected() {
    let (_temp, app) = app_with_history().await;
    let (status, body) = get(&app, "/api/v1/sessions/search?q=").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

/// `limit` caps the hits, and `session_id` narrows the search.
#[tokio::test]
async fn limit_and_session_filter_are_honoured() {
    let (_temp, app) = app_with_history().await;
    let (status, body) =
        get(&app, "/api/v1/sessions/search?q=tokens&session_id=other").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"].as_array().map(Vec::len),
        Some(0),
        "another session must not match: {body}"
    );

    let (status, body) =
        get(&app, "/api/v1/sessions/search?q=tokens&limit=1").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"].as_array().map(Vec::len), Some(1), "{body}");
    // The limit was reached, so the total is withheld (it would be a lie).
    assert!(body.get("total").is_none(), "{body}");
}
