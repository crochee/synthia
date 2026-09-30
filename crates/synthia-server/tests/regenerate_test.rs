//! The two chat routes whose *target session* was hardest to get
//! right: `regenerate` (which must find the turn to replay) and
//! `feedback` (which must record into the right log).
//!
//! The contract is documented on `routes::chat::regenerate`: re-queue
//! the most recent user turn, recovered from the session log. The
//! handler falls through to `SessionOp::Cancel` when it cannot find
//! one, which means a broken recovery step does not fail loudly — it
//! silently turns "regenerate" into "stop". These tests pin the
//! recovery against the rows the running system actually writes.
//!
//! Deterministic: the fixture's `AppState::for_test` wires a
//! `FakeProvider`, so no run touches the network.

use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use synthia::session::manager::{SERVER_DEFAULT_USER_ID, SessionRegistry};
use synthia_server::{create_router, state::AppState};
use tower::ServiceExt;

struct Fixture {
    app: axum::Router,
    _temp: tempfile::TempDir,
    state: Arc<AppState>,
}

async fn make_fixture() -> Fixture {
    let temp = tempfile::TempDir::new().unwrap();
    let sessions_root = temp.path().to_path_buf();
    let registry = SessionRegistry::new(sessions_root.clone());
    let state =
        Arc::new(AppState::for_test(registry, sessions_root.clone()).await);
    let app = create_router(Arc::clone(&state)).await;
    Fixture {
        app,
        _temp: temp,
        state,
    }
}

async fn seed_session(fix: &Fixture, session_id: &str) {
    let req = Request::builder()
        .uri("/api/v1/chat/sessions".to_string())
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(json!({"session_id": session_id}).to_string()))
        .unwrap();
    let resp = fix.app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "create_session must seed the controller"
    );
}

async fn append_rows(fix: &Fixture, session_id: &str, rows: &[Value]) {
    let sink = fix
        .state
        .session_manager
        .sink(SERVER_DEFAULT_USER_ID, session_id);
    for row in rows {
        sink.append(row).await.unwrap();
    }
}

async fn read_rows(fix: &Fixture, session_id: &str) -> Vec<Value> {
    fix.state
        .session_manager
        .sink(SERVER_DEFAULT_USER_ID, session_id)
        .read()
        .await
        .unwrap()
}

fn user_input_texts(rows: &[Value]) -> Vec<String> {
    rows.iter()
        .filter(|r| r.get("type").and_then(Value::as_str) == Some("UserInput"))
        .filter_map(|r| {
            r.get("data")
                .and_then(|d| d.get("text"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect()
}

async fn post_regenerate(fix: &Fixture, session_id: &str) -> StatusCode {
    let req = Request::builder()
        .uri(format!("/api/v1/chat/sessions/{session_id}/regenerate"))
        .method("POST")
        .body(Body::empty())
        .unwrap();
    fix.app.clone().oneshot(req).await.unwrap().status()
}

/// The rows this test writes are the ones the controller actually
/// emits for a turn (`{"type":"UserInput","data":{"text":…}}`). A
/// recovery step that matches any other shape finds nothing in a real
/// log, and the handler then cancels instead of rerunning — the
/// failure this pins.
///
/// The observable is the log itself: a re-run goes through the run
/// task, which persists the recovered turn as a fresh `UserInput` row
/// before sampling. `SessionOp::Cancel` appends nothing, so a second
/// row can only mean the turn was genuinely replayed.
#[tokio::test]
async fn regenerate_replays_the_last_user_turn_from_a_real_log() {
    let fix = make_fixture().await;
    let session_id = "regenerate-real-shape";
    seed_session(&fix, session_id).await;

    append_rows(
        &fix,
        session_id,
        &[
            json!({"type": "UserInput", "data": {"text": "first turn"}}),
            json!({
                "type": "Model",
                "data": {"type": "text", "text": "first answer"},
            }),
            json!({"type": "UserInput", "data": {"text": "second turn"}}),
        ],
    )
    .await;
    assert_eq!(
        user_input_texts(&read_rows(&fix, session_id).await),
        vec!["first turn".to_string(), "second turn".to_string()],
        "precondition: two user rows"
    );

    assert_eq!(
        post_regenerate(&fix, session_id).await,
        StatusCode::ACCEPTED
    );

    // The rerun is asynchronous — poll for its row.
    let mut texts = Vec::new();
    for _ in 0..100 {
        texts = user_input_texts(&read_rows(&fix, session_id).await);
        if texts.len() >= 3 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    assert_eq!(
        texts,
        vec![
            "first turn".to_string(),
            "second turn".to_string(),
            "second turn".to_string(),
        ],
        "regenerate must re-queue the MOST RECENT user turn; a cancel \
         would append nothing"
    );
}

/// A session whose log holds no user turn has nothing to replay. The
/// handler must report that miss — NOT answer it with `Cancel`, which
/// is the op `/cancel` submits and would abort the session as a side
/// effect of a failed read.
#[tokio::test]
async fn regenerate_reports_a_missing_user_turn_instead_of_cancelling() {
    let fix = make_fixture().await;
    let session_id = "regenerate-no-user-turn";
    seed_session(&fix, session_id).await;

    append_rows(
        &fix,
        session_id,
        &[json!({
            "type": "Model",
            "data": {"type": "text", "text": "orphan answer"},
        })],
    )
    .await;

    assert_eq!(
        post_regenerate(&fix, session_id).await,
        StatusCode::NOT_FOUND,
        "a miss must be reported, not turned into a session cancel"
    );
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let rows = read_rows(&fix, session_id).await;
    assert!(
        user_input_texts(&rows).is_empty(),
        "nothing may be invented; got {rows:?}"
    );
    // The decisive check: the session is still usable. Under the
    // old `None => Cancel` arm the controller would be Cancelled.
    let req = Request::builder()
        .uri("/api/v1/chat/sessions")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(json!({"session_id": session_id}).to_string()))
        .unwrap();
    assert_eq!(
        fix.app.clone().oneshot(req).await.unwrap().status(),
        StatusCode::OK,
        "the session must survive a regenerate miss"
    );
}

/// A session that exists but whose log was never written has no
/// `events.jsonl`, so the read itself fails. That is still a miss, not
/// a server fault: it must answer `404`, and (because the controller
/// is resolved *after* the read) it must not leave a session behind
/// via the eager-create path either.
#[tokio::test]
async fn regenerate_reports_a_fresh_session_without_creating_one() {
    let fix = make_fixture().await;
    let session_id = "regenerate-never-written";
    seed_session(&fix, session_id).await;

    // Deliberately append nothing: the controller exists, the log
    // file does not.
    assert_eq!(
        post_regenerate(&fix, session_id).await,
        StatusCode::NOT_FOUND,
        "an unreadable/absent log is a miss, not a 500"
    );
}

/// The multimodal half: a turn sent with an attachment persists only
/// its text (the bytes are deliberately not serialised), so regenerate
/// replays that text. This pins the composition of the two behaviours
/// — prompt-text persistence and turn recovery — because a regenerate
/// that saw an attachment-only payload would replay nothing.
#[tokio::test]
async fn regenerate_replays_the_text_of_a_multimodal_turn() {
    let fix = make_fixture().await;
    let session_id = "regenerate-multimodal";
    seed_session(&fix, session_id).await;

    append_rows(
        &fix,
        session_id,
        &[json!({
            "type": "UserInput",
            "data": {"text": "what is in this picture?"},
        })],
    )
    .await;

    assert_eq!(
        post_regenerate(&fix, session_id).await,
        StatusCode::ACCEPTED
    );

    let mut texts = Vec::new();
    for _ in 0..100 {
        texts = user_input_texts(&read_rows(&fix, session_id).await);
        if texts.len() >= 2 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(texts.len(), 2, "the prompt text must be replayed");
    assert_eq!(texts[0], texts[1], "and it must be the same text");
}

// ---------------------------------------------------------------------------
// POST /api/v1/chat/messages/{message_id}/feedback
// ---------------------------------------------------------------------------

async fn post_feedback(
    fix: &Fixture,
    message_id: &str,
    session_id: &str,
    thumbs_up: bool,
) -> StatusCode {
    let req = Request::builder()
        .uri(format!("/api/v1/chat/messages/{message_id}/feedback"))
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"thumbs_up": thumbs_up, "session_id": session_id})
                .to_string(),
        ))
        .unwrap();
    fix.app.clone().oneshot(req).await.unwrap().status()
}

fn feedback_rows(rows: &[Value]) -> Vec<Value> {
    rows.iter()
        .filter(|r| r.get("kind").and_then(Value::as_str) == Some("feedback"))
        .cloned()
        .collect()
}

/// A rating must land in the session the client names.
///
/// The handler previously resolved a hardcoded `"default"` session,
/// so every 👍/👎 in every session was recorded against a session that
/// does not exist — and created it on the first click.
#[tokio::test]
async fn feedback_records_into_the_named_session() {
    let fix = make_fixture().await;
    seed_session(&fix, "session-rated").await;

    assert_eq!(
        post_feedback(&fix, "msg-1", "session-rated", true).await,
        StatusCode::NO_CONTENT
    );

    // The append happens on the controller's task, and the log file
    // does not exist until the first row lands — so poll tolerantly.
    let mut feedback = Vec::new();
    for _ in 0..100 {
        let rows = fix
            .state
            .session_manager
            .sink(SERVER_DEFAULT_USER_ID, "session-rated")
            .read()
            .await
            .unwrap_or_default();
        feedback = feedback_rows(&rows);
        if !feedback.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(feedback.len(), 1, "one rating; got {feedback:?}");
    assert_eq!(feedback[0]["message_id"], "msg-1");
    assert_eq!(feedback[0]["thumbs_up"], true);
}

/// The session id comes from the request body and reaches the sink's
/// filesystem path, so a traversal attempt must be rejected — not
/// silently used to build a directory.
#[tokio::test]
async fn feedback_rejects_a_traversing_session_id() {
    let fix = make_fixture().await;
    for bad in ["../../etc", "a/b", "."] {
        let status = post_feedback(&fix, "msg-1", bad, true).await;
        assert!(
            status.is_client_error(),
            "session_id {bad:?} must be rejected, got {status}"
        );
    }
}
