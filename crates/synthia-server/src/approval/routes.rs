use std::sync::Arc;

use axum::{
    Json,
    extract::{
        Path,
        State,
        WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::StatusCode,
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use synthia_permission::ApprovalOutcome;

use super::state::ApprovalEvent;
use crate::{
    api::{ApiError, ErrorDetail, json_data},
    state::AppState,
};

/// Request body for resolving an approval.
#[derive(Debug, Deserialize)]
pub struct ResolveApprovalRequest {
    pub outcome: String,
}

/// Response body for a successful resolution.
#[derive(Debug, Serialize, Deserialize)]
pub struct ResolveApprovalResponse {
    pub resolved: bool,
}

/// GET /api/approvals - List all pending approval requests.
pub async fn list_approvals(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, ApiError> {
    let approvals = state.approval_state.list_pending();
    Ok(json_data(approvals))
}

/// POST /api/approvals/:id/resolve - Resolve a pending approval request.
pub async fn resolve_approval(
    State(state): State<Arc<AppState>>,
    Path(request_id): Path<String>,
    Json(req): Json<ResolveApprovalRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let outcome = match req.outcome.to_ascii_lowercase().as_str() {
        "approve" => ApprovalOutcome::Approve,
        "deny" => ApprovalOutcome::Deny,
        _ => {
            return Err(ApiError::validation_error(vec![ErrorDetail::new(
                Some("outcome"),
                "outcome must be 'approve' or 'deny'",
                "invalid_outcome",
            )]));
        }
    };

    if state.approval_state.resolve(&request_id, outcome) {
        Ok((
            StatusCode::OK,
            json_data(ResolveApprovalResponse { resolved: true }),
        )
            .into_response())
    } else {
        Err(ApiError::not_found("approval request"))
    }
}

/// GET /ws/approvals - WebSocket stream of approval events.
pub async fn ws_approvals_handler(
    State(state): State<Arc<AppState>>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_approvals_websocket(socket, state))
}

async fn handle_approvals_websocket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();
    let mut event_rx = state.approval_state.subscribe();

    // Send a snapshot of current pending approvals on connect.
    let snapshot = ApprovalEvent::Snapshot {
        approvals: state.approval_state.list_pending(),
    };
    let snapshot_text = serde_json::to_string(&snapshot).unwrap_or_default();
    let _ = sender.send(Message::Text(snapshot_text.into())).await;

    let send_task = tokio::spawn(async move {
        while let Ok(event) = event_rx.recv().await {
            let text = serde_json::to_string(&event).unwrap_or_default();
            if sender.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    while let Some(Ok(msg)) = receiver.next().await {
        if matches!(msg, Message::Close(_)) {
            break;
        }
    }

    send_task.abort();
}
