//! V2 Server-Sent Events endpoint for session event history + live stream.
//!
//! GET /api/v2/sessions/{id}/events?last_seq={N}
//!
//! The endpoint replays persisted events with `seq > last_seq`, emits a
//! synthetic `SyncCaughtUp` event, then continues streaming live events
//! from the session controller's broadcast channel.

use std::sync::Arc;

use axum::{
    Extension,
    extract::{Path, Query, State},
    response::{
        IntoResponse,
        sse::{Event as SseEvent, KeepAlive, Sse},
    },
};
use futures::StreamExt;

use super::models::EventsQuery;
use crate::{api::ApiError, middleware::auth::RequestUserId, state::AppState};

/// GET /api/v2/sessions/:id/events - Replay history and stream live events.
pub async fn session_events(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<RequestUserId>,
    Path(session_id): Path<String>,
    Query(query): Query<EventsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    // Verify ownership first so existence is not leaked.
    state
        .session_manager
        .get_for_user(user_id.as_str(), &session_id)
        .await
        .map_err(ApiError::from)?;

    // Ensure the controller (and therefore its broadcaster) exists.
    let controller = state
        .get_or_create_session_controller(user_id.as_str(), &session_id)
        .await
        .map_err(|e| ApiError::internal(format!("{:?}", e)))?;

    let session_path = state
        .session_manager
        .store()
        .session_dir(user_id.as_str(), &session_id);
    let last_seq = query.last_seq;

    // File I/O is performed on a blocking task.
    let replay_events = tokio::task::spawn_blocking(move || {
        let store = synthia_session::store::EventStore::new();
        store
            .read_from(&session_path, last_seq, usize::MAX)
            .map_err(|e| ApiError::internal(e.to_string()))
    })
    .await
    .map_err(|e| ApiError::internal(e.to_string()))??;

    let mut replay_last_seq = last_seq;
    let mut initial_events = Vec::new();
    for event in replay_events {
        replay_last_seq = event.seq;
        let data = serde_json::to_string(&event).unwrap_or_default();
        initial_events
            .push(SseEvent::default().event(&event.event_type).data(data));
    }

    let caught_up = serde_json::json!({
        "last_seq": replay_last_seq,
        "replay_count": initial_events.len(),
    });
    initial_events.push(
        SseEvent::default()
            .event("SyncCaughtUp")
            .data(caught_up.to_string()),
    );

    let rx = controller.subscribe();

    let stream = async_stream::stream! {
        for event in initial_events {
            yield Ok::<_, std::convert::Infallible>(event);
        }

        let mut live = tokio_stream::wrappers::BroadcastStream::new(rx);
        while let Some(result) = live.next().await {
            match result {
                Ok(agent_event) => {
                    yield Ok(crate::sse::agent_event_to_sse(&agent_event));
                }
                Err(_) => break,
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(crate::sse::HEARTBEAT_INTERVAL)
            .text(": ping"),
    ))
}
