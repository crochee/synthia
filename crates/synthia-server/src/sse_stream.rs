//! SSE streaming endpoint for session events.
//!
//! GET /api/v2/sessions/:id/stream-sse
//! Returns Server-Sent Events for an active agent session.

use std::sync::Arc;

use axum::{
    Extension,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};

use crate::{
    middleware::auth::RequestUserId,
    sse::sse_event_stream,
    state::AppState,
};

/// GET /api/v2/sessions/:id/stream-sse
///
/// Streams agent events for the given session as Server-Sent Events.
/// The client must already have an active session; this endpoint subscribes
/// to the event broadcast channel and forwards events in real-time.
pub async fn stream_sse_handler(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<RequestUserId>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    // Verify session exists
    if state.session_manager.get(&session_id).await.is_none() {
        return (
            StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({
                "error": "not_found",
                "message": format!("Session '{}' not found", session_id)
            })),
        )
            .into_response();
    }

    // Try to get the event broadcaster for this session
    let broadcaster = match state
        .get_event_broadcaster(user_id.as_str(), &session_id)
        .await
    {
        Some(b) => b,
        None => {
            return (
                StatusCode::GONE,
                axum::Json(serde_json::json!({
                    "error": "gone",
                    "message": "Session event stream is no longer available"
                })),
            )
                .into_response();
        }
    };

    let rx = broadcaster.subscribe();
    sse_event_stream(rx).into_response()
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_accept_header_sse_detection() {
        assert!(
            Some("text/event-stream".to_string())
                .as_deref()
                .map(|h| h.contains("text/event-stream"))
                .unwrap_or(false)
        );

        assert!(
            !Some("application/json".to_string())
                .as_deref()
                .map(|h| h.contains("text/event-stream"))
                .unwrap_or(false)
        );

        assert!(
            !None::<String>
                .as_deref()
                .map(|h| h.contains("text/event-stream"))
                .unwrap_or(false)
        );
    }
}
