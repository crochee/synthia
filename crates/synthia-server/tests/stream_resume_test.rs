//! End-to-end tests for the resume seam on the chat SSE
//! stream.
//!
//! The contract is documented on
//! `routes::chat::stream_messages`:
//!
//! - `GET /api/v1/chat/sessions/{id}/messages/stream`
//!   without `?from` is byte-identical to today: every
//!   persisted event is replayed and the live tail is
//!   forwarded as `data: <json>` frames (no named
//!   `event:` line).
//! - With `?from = N` the wire shape is:
//!
//! ```text
//! event: snapshot
//! data: {"last_event_seq": <u64>}
//!
//! event: message
//! data: <event 1>
//!
//! ...
//!
//! event: cursor
//! data: {"last_event_seq": <u64>}
//!
//! event: message
//! data: <live event>
//! ```
//!
//! - `?from = N` past `last_event_seq` returns `410 Gone` with
//!   a body that carries `current_last_event_seq` so the
//!   client learns to restart.
//! - `?from = abc` (unparseable) returns `400`.
//!
//! Each scenario writes a known sequence of synthetic
//! events to the sink via `SessionSink::append` — the same
//! path `persist_and_broadcast` uses in production. The
//! tests construct their own `SessionRegistry` rooted at
//! the temp dir so they can interleave writes with the
//! router.

use std::{collections::HashMap, sync::Arc};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use synthia::session::manager::SessionRegistry;
use synthia_server::{create_router, state::AppState};
use tower::ServiceExt;
/// Shared test fixture: temp dir + router + registry handle.
struct Fixture {
    app: axum::Router,
    /// Hold the temp dir so it is not dropped while the
    /// router reads from the on-disk JSONL files.
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
        "create_session must seed the controller; got {}",
        resp.status()
    );
}

async fn write_events(
    fix: &Fixture,
    session_id: &str,
    events: &[Value],
) -> u64 {
    let sink = fix.state.session_manager.sink(
        synthia::session::manager::SERVER_DEFAULT_USER_ID,
        session_id,
    );
    for ev in events {
        sink.append(ev).await.unwrap();
    }
    let snap = sink.snapshot().await.unwrap();
    drop(sink);
    snap.last_event_seq
}

#[derive(Debug)]
struct SseFrame {
    event: String,
    data: String,
}

impl SseFrame {
    fn json(&self) -> Value {
        serde_json::from_str(&self.data).unwrap_or_else(|e| {
            panic!(
                "frame `{}`: expected JSON data, got {:?} (parse error: {e})",
                self.event, self.data
            )
        })
    }
}

fn frame_kinds(frames: &[SseFrame]) -> HashMap<&str, usize> {
    let mut kinds: HashMap<&str, usize> = HashMap::new();
    for f in frames {
        *kinds.entry(f.event.as_str()).or_insert(0) += 1;
    }
    kinds
}

/// Parse the SSE frames out of an `axum::body::Body`.
/// The body is collected whole (the router terminates the
/// stream once the broadcaster idles) so the SSE grammar is
/// a single `\n\n`-separated block list.
/// Collect SSE frames with a short timeout. The route's
/// SSE stream only terminates when a `SessionEnded`
/// arrives on the broadcast or the broadcaster is dropped;
/// in these tests we never run an agent, so the
/// stream never closes on its own. The timeout bounds the
/// wait — by then we have enough bytes to assert on.
async fn collect_sse_frames(body: Body) -> Vec<SseFrame> {
    use std::time::Duration;

    use futures::StreamExt;
    // `body.collect()` waits for the stream to close, but the
    // route's live tail only closes on a `SessionEnded`. We
    // stream frames one-by-one with a short idle timeout: the
    // resume path yields the snapshot/replay/cursor prefix
    // immediately, and we collect whatever the server emits
    // in that window.
    let mut stream = body.into_data_stream();
    let mut buf: Vec<u8> = Vec::new();
    loop {
        match tokio::time::timeout(Duration::from_secs(2), stream.next()).await
        {
            Ok(Some(Ok(chunk))) => buf.extend_from_slice(&chunk),
            Ok(Some(Err(_))) | Ok(None) => break,
            Err(_) => break,
        }
    }
    let mut frames = Vec::new();
    let mut current: Option<SseFrame> = None;
    for raw in buf.split(|b| *b == b'\n') {
        if raw.is_empty() {
            if let Some(frame) = current.take() {
                frames.push(frame);
            }
            continue;
        }
        if let Some(rest) = raw.strip_prefix(b"event: ") {
            current = Some(SseFrame {
                event: String::from_utf8_lossy(rest).into_owned(),
                data: String::new(),
            });
            continue;
        }
        if let Some(rest) = raw.strip_prefix(b"data: ") {
            let entry = current.get_or_insert_with(|| SseFrame {
                event: String::new(),
                data: String::new(),
            });
            if !entry.data.is_empty() {
                entry.data.push('\n');
            }
            entry.data.push_str(&String::from_utf8_lossy(rest));
            continue;
        }
        // ignore SSE comment / heartbeat lines.
    }
    frames
}
fn message_payloads(frames: &[SseFrame]) -> Vec<Value> {
    frames
        .iter()
        .filter(|f| f.event == "message")
        .map(|f| f.json())
        .collect()
}

/// `?from` is absent. The stream must NOT carry named
/// `GET /api/v1/chat/sessions/{id}/messages/stream`
/// without `?from` now always takes the resume path — the
/// protocol's `stream-index` design requires every client
/// to receive a `snapshot` and a `cursor` frame so a
/// reconnecting client can learn the current cursor before
/// the live tail starts. The new wire shape is documented
/// in `2026-09-24-stream-index.md` §4.4.
#[tokio::test]
async fn stream_without_from_emits_snapshot_and_cursor() {
    let fix = make_fixture().await;
    let session_id = "no-from";
    seed_session(&fix, session_id).await;
    write_events(
        &fix,
        session_id,
        &[
            json!({"type": "Model", "data": {"text": "first"}}),
            json!({"type": "Model", "data": {"text": "second"}}),
            json!({"type": "System", "data": {"kind": "SessionStarted", "session_id": session_id}}),
        ],
    )
    .await;

    let req = Request::builder()
        .uri(format!(
            "/api/v1/chat/sessions/{session_id}/messages/stream"
        ))
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let resp = fix.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let frames = collect_sse_frames(resp.into_body()).await;
    let kinds = frame_kinds(&frames);
    // One snapshot frame at the head, one cursor frame
    // after the replay. Both must carry `stream_index`.
    assert_eq!(kinds.get("snapshot").copied(), Some(1));
    assert_eq!(kinds.get("cursor").copied(), Some(1));
    let snapshot = frames
        .iter()
        .find(|f| f.event == "snapshot")
        .expect("snapshot frame present")
        .json();
    let cursor = frames
        .iter()
        .find(|f| f.event == "cursor")
        .expect("cursor frame present")
        .json();
    assert_eq!(
        snapshot.get("stream_index").and_then(Value::as_u64),
        Some(3),
        "snapshot.stream_index == last sink ordinal",
    );
    assert_eq!(
        cursor.get("stream_index").and_then(Value::as_u64),
        Some(3),
        "cursor.stream_index == last sink ordinal",
    );
}

/// `?from = 0` takes the same resume path as no `from` —
/// the replay starts at index 1, snapshot + cursor carry
/// `stream_index == 3`.
#[tokio::test]
async fn stream_with_from_zero_emits_snapshot_and_cursor() {
    let fix = make_fixture().await;
    let session_id = "from-zero";
    seed_session(&fix, session_id).await;
    write_events(
        &fix,
        session_id,
        &(0..3)
            .map(
                |i| json!({"type": "Model", "data": {"text": format!("t{i}")}}),
            )
            .collect::<Vec<_>>(),
    )
    .await;

    let req = Request::builder()
        .uri(format!(
            "/api/v1/chat/sessions/{session_id}/messages/stream?from=0"
        ))
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let resp = fix.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let frames = collect_sse_frames(resp.into_body()).await;
    let kinds = frame_kinds(&frames);
    assert_eq!(kinds.get("snapshot").copied(), Some(1));
    assert_eq!(kinds.get("cursor").copied(), Some(1));
}

/// A freshly seeded session has no events.jsonl on disk yet —
/// the unified resume path must treat the missing file as an
/// empty replay rather than a 500, so a client that opens the
/// stream before the first turn still learns its cursor (0).
#[tokio::test]
async fn stream_on_fresh_session_emits_zero_cursor() {
    let fix = make_fixture().await;
    let session_id = "fresh-session";
    seed_session(&fix, session_id).await;
    // Deliberately NO write_events: the sink has never appended.

    let req = Request::builder()
        .uri(format!(
            "/api/v1/chat/sessions/{session_id}/messages/stream"
        ))
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let resp = fix.app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "fresh session (no events.jsonl) must stream, not 500"
    );
    let frames = collect_sse_frames(resp.into_body()).await;
    let kinds = frame_kinds(&frames);
    assert_eq!(kinds.get("snapshot").copied(), Some(1));
    assert_eq!(kinds.get("cursor").copied(), Some(1));
    let cursor = frames
        .iter()
        .find(|f| f.event == "cursor")
        .expect("cursor frame present")
        .json();
    assert_eq!(
        cursor.get("stream_index").and_then(Value::as_u64),
        Some(0),
        "fresh session cursor.stream_index == 0",
    );
}

/// `?from = N` replays exactly the events whose seq is
/// strictly greater than `N`, in chronological order. The
/// wire shape includes a `snapshot` frame, one `message`
/// frame per replayed event, then a `cursor` frame.
#[tokio::test]
async fn stream_with_from_n_replays_only_events_strictly_after() {
    let fix = make_fixture().await;
    let session_id = "from-n";
    seed_session(&fix, session_id).await;
    let last = write_events(
        &fix,
        session_id,
        &(0..5)
            .map(
                |i| json!({"type": "Model", "data": {"text": format!("t{i}")}}),
            )
            .collect::<Vec<_>>(),
    )
    .await;
    assert_eq!(last, 5);

    let req = Request::builder()
        .uri(format!(
            "/api/v1/chat/sessions/{session_id}/messages/stream?from=2"
        ))
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let resp = fix.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let frames = collect_sse_frames(resp.into_body()).await;

    // First frame: snapshot carrying last_event_seq.
    let snapshot_frame = frames
        .iter()
        .find(|f| f.event == "snapshot")
        .expect("resume stream must carry a snapshot frame");
    assert_eq!(
        snapshot_frame.json()["last_event_seq"],
        json!(5),
        "snapshot frame must reflect last_event_seq at the time of subscribe"
    );

    // Replay messages: exactly seq 3, 4, 5 (three events).
    let parsed = message_payloads(&frames);
    assert_eq!(
        parsed.len(),
        3,
        "from=2 must replay exactly seq 3, 4, 5 (3 events); got {parsed:?}"
    );
    assert_eq!(parsed[0]["data"]["text"], "t2");
    assert_eq!(parsed[1]["data"]["text"], "t3");
    assert_eq!(parsed[2]["data"]["text"], "t4");

    // Post-replay cursor.
    let cursor_frame = frames
        .iter()
        .find(|f| f.event == "cursor")
        .expect("resume stream must carry a cursor frame after the replay");
    assert_eq!(cursor_frame.json()["last_event_seq"], json!(5));

    // Frame ordering: snapshot → message* → cursor.
    let order: Vec<&str> = frames.iter().map(|f| f.event.as_str()).collect();
    let snap_idx = order.iter().position(|e| *e == "snapshot").unwrap();
    let cursor_idx = order.iter().position(|e| *e == "cursor").unwrap();
    let first_message = order
        .iter()
        .position(|e| *e == "message")
        .unwrap_or(usize::MAX);
    assert!(
        snap_idx < first_message && first_message < cursor_idx,
        "message frames must sit between snapshot ({snap_idx}) and cursor ({cursor_idx}); order={order:?}"
    );
}

/// `?from > last_event_seq` returns `410 Gone` with a body
/// that carries `current_last_event_seq`. A reconnecting
/// client learns it has been left behind by the live tail
/// and can restart from scratch.
#[tokio::test]
async fn stream_with_from_past_end_returns_410_with_cursor() {
    let fix = make_fixture().await;
    let session_id = "stale-cursor";
    seed_session(&fix, session_id).await;
    write_events(
        &fix,
        session_id,
        &(0..3)
            .map(
                |i| json!({"type": "Model", "data": {"text": format!("t{i}")}}),
            )
            .collect::<Vec<_>>(),
    )
    .await;

    let req = Request::builder()
        .uri(format!(
            "/api/v1/chat/sessions/{session_id}/messages/stream?from=999"
        ))
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let resp = fix.app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::GONE,
        "from past last_event_seq must be 410 Gone"
    );
    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["code"], json!("resume_cursor_truncated"));
    assert_eq!(
        body["current_last_event_seq"],
        json!(3),
        "the 410 body must carry the current cursor so the client can restart"
    );
}

/// `?from = abc` is unparseable → `400 Bad Request`. The
/// handler returns a custom `invalid_resume_cursor` error
/// envelope.
#[tokio::test]
async fn stream_with_unparseable_from_returns_400() {
    let fix = make_fixture().await;
    let session_id = "bad-cursor";
    seed_session(&fix, session_id).await;

    let req = Request::builder()
        .uri(format!(
            "/api/v1/chat/sessions/{session_id}/messages/stream?from=abc"
        ))
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let resp = fix.app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "unparseable `from` must be 400"
    );
    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["code"], json!("invalid_resume_cursor"));
}

/// Negative cursors reject the request at parse time. An
/// empty `from=` is treated as "from the start" so the
/// stream-index protocol's snapshot + cursor frames are
/// always emitted -- `from = ""` is indistinguishable from
/// `from = None`.
#[tokio::test]
async fn stream_with_negative_or_empty_from_behaves_consistently() {
    let fix = make_fixture().await;
    let session_id = "neg-cursor";
    seed_session(&fix, session_id).await;
    write_events(
        &fix,
        session_id,
        &[json!({"type": "Model", "data": {"text": "x"}})],
    )
    .await;

    let req = Request::builder()
        .uri(format!(
            "/api/v1/chat/sessions/{session_id}/messages/stream?from=-1"
        ))
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let resp = fix.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // `from = ""` and `from = None` both walk the resume
    // path: the client sees a snapshot + cursor frame, the
    // wire surface for the live cursor.
    let req = Request::builder()
        .uri(format!(
            "/api/v1/chat/sessions/{session_id}/messages/stream?from="
        ))
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let resp = fix.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let frames = collect_sse_frames(resp.into_body()).await;
    let kinds = frame_kinds(&frames);
    assert_eq!(kinds.get("snapshot").copied(), Some(1));
    assert_eq!(kinds.get("cursor").copied(), Some(1));
}

/// `?from = last_event_seq` replays zero events — the
/// snapshot and cursor frames are still emitted so the
/// client learns the live position.
#[tokio::test]
async fn stream_with_from_equal_to_last_event_seq_replays_empty() {
    let fix = make_fixture().await;
    let session_id = "exact-end";
    seed_session(&fix, session_id).await;
    let last = write_events(
        &fix,
        session_id,
        &(0..4)
            .map(
                |i| json!({"type": "Model", "data": {"text": format!("t{i}")}}),
            )
            .collect::<Vec<_>>(),
    )
    .await;
    assert_eq!(last, 4);

    let req = Request::builder()
        .uri(format!(
            "/api/v1/chat/sessions/{session_id}/messages/stream?from=4"
        ))
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let resp = fix.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let frames = collect_sse_frames(resp.into_body()).await;
    let parsed = message_payloads(&frames);
    assert!(
        parsed.is_empty(),
        "from == last_event_seq must replay zero message frames; got {parsed:?}"
    );
    let kinds = frame_kinds(&frames);
    assert_eq!(kinds.get("snapshot").copied().unwrap_or(0), 1);
    assert!(kinds.get("cursor").copied().unwrap_or(0) >= 1);
}

/// `?from = N` is a *strict* suffix — events with seq equal
/// to `N` are NOT replayed.
#[tokio::test]
async fn stream_from_n_excludes_seq_equal_to_n() {
    let fix = make_fixture().await;
    let session_id = "strict-suffix";
    seed_session(&fix, session_id).await;
    write_events(
        &fix,
        session_id,
        &(0..3)
            .map(
                |i| json!({"type": "Model", "data": {"text": format!("t{i}")}}),
            )
            .collect::<Vec<_>>(),
    )
    .await;

    // from = 1 → only seq 2, 3 (two events).
    let req = Request::builder()
        .uri(format!(
            "/api/v1/chat/sessions/{session_id}/messages/stream?from=1"
        ))
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let resp = fix.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let frames = collect_sse_frames(resp.into_body()).await;
    let parsed = message_payloads(&frames);
    assert_eq!(parsed.len(), 2, "from=1 must replay exactly seq 2, 3");
    assert_eq!(parsed[0]["data"]["text"], "t1");
    assert_eq!(parsed[1]["data"]["text"], "t2");
}

/// The wire cursor schema is stable: `last_event_seq` is a
/// JSON number, so a client can parse it directly.
#[tokio::test]
async fn stream_cursor_frame_carries_numeric_last_event_seq() {
    let fix = make_fixture().await;
    let session_id = "numeric-cursor";
    seed_session(&fix, session_id).await;
    write_events(
        &fix,
        session_id,
        &(0..7)
            .map(|_| json!({"type": "Model", "data": {"text": "x"}}))
            .collect::<Vec<_>>(),
    )
    .await;

    let req = Request::builder()
        .uri(format!(
            "/api/v1/chat/sessions/{session_id}/messages/stream?from=4"
        ))
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let resp = fix.app.clone().oneshot(req).await.unwrap();
    let frames = collect_sse_frames(resp.into_body()).await;
    let snapshot_frame = frames.iter().find(|f| f.event == "snapshot").unwrap();
    let cursor_frame = frames.iter().find(|f| f.event == "cursor").unwrap();
    let snap_val = &snapshot_frame.json()["last_event_seq"];
    let cursor_val = &cursor_frame.json()["last_event_seq"];
    assert!(
        snap_val.is_u64() || snap_val.is_i64(),
        "snapshot.last_event_seq must be a JSON number; got {snap_val:?}"
    );
    assert!(
        cursor_val.is_u64() || cursor_val.is_i64(),
        "cursor.last_event_seq must be a JSON number; got {cursor_val:?}"
    );
    assert_eq!(snap_val, cursor_val);
    assert_eq!(*snap_val, json!(7));
}
