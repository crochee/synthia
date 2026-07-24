use std::sync::Arc;

use axum::{
    Json,
    extract::{
        Path,
        Query,
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

/// Query parameters for the WebSocket upgrade request.
#[derive(Debug, Deserialize)]
pub struct WsUpgradeQuery {
    /// API key passed via `?token=xxx` (WebSocket clients cannot set
    /// arbitrary headers in browsers). The token is validated against
    /// the configured `SYNTHIA_API_KEY` (or, if absent, against the
    /// server's `auth.api_keys` list) before the WebSocket upgrade is
    /// accepted.
    #[serde(default)]
    pub token: Option<String>,
}

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
///
/// Authentication is performed via the `?token=xxx` query parameter
/// because browser WebSocket clients cannot set arbitrary request
/// headers. The token is validated against the configured
/// `SYNTHIA_API_KEY` (env) and, falling back to the server's
/// `auth.api_keys` list when the env var is unset. When no API key
/// is configured at all, the connection is accepted (matches the
/// behavior of the HTTP [`AuthLayer`] for unconfigured deployments).
pub async fn ws_approvals_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<WsUpgradeQuery>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    if !validate_ws_token(query.token.as_deref(), &state) {
        return (
            StatusCode::UNAUTHORIZED,
            "Invalid or missing authentication credentials",
        )
            .into_response();
    }
    ws.on_upgrade(move |socket| handle_approvals_websocket(socket, state))
}

/// Validate the token supplied in the WebSocket upgrade query string.
///
/// Mirrors the same posture as the HTTP [`AuthLayer`]:
/// - If no API key is configured (env + config both empty), accept.
/// - Otherwise, the supplied token must match the configured key.
fn validate_ws_token(token: Option<&str>, state: &AppState) -> bool {
    let expected = expected_api_key(state);
    match expected {
        None => true,
        Some(expected) => token.is_some_and(|t| t == expected),
    }
}

/// Resolve the expected API key from the `SYNTHIA_API_KEY` env var,
/// falling back to the first non-empty entry in `auth.api_keys`.
/// Returns `None` (meaning "no auth configured") if both sources
/// are empty.
fn expected_api_key(state: &AppState) -> Option<String> {
    if let Ok(env_key) = std::env::var("SYNTHIA_API_KEY")
        && !env_key.is_empty()
    {
        return Some(env_key);
    }
    state
        .auth_config
        .api_keys
        .iter()
        .find(|k| !k.is_empty())
        .cloned()
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{config::AuthConfig, state::AppState};

    /// Test-only helper: build an `AppState` whose relevant fields
    /// (`auth_config`) we can directly mutate. We reach for the
    /// placeholder fields by going through `for_test` and then
    /// overwriting the `Arc<AuthConfig>` with the desired value.
    async fn state_with_auth(keys: Vec<String>) -> AppState {
        let tmp = tempfile::tempdir().expect("tempdir");
        use synthia_session::manager::SessionManager;
        let sm = SessionManager::new(tmp.path().join("sessions"));
        let mut state =
            AppState::for_test(sm, tmp.path().join("workspace")).await;
        state.auth_config = Arc::new(AuthConfig {
            enabled: true,
            api_keys: keys,
            key_to_user: std::collections::HashMap::new(),
        });
        state
    }

    /// When no API key is configured at all, the validator should
    /// accept every connection (mirrors the HTTP AuthLayer behavior).
    #[tokio::test]
    #[serial_test::serial]
    async fn validate_ws_token_accepts_when_no_key_configured() {
        // Save original env var and restore after test
        let original = std::env::var("SYNTHIA_API_KEY").ok();
        // SAFETY: test runs on dedicated task, env restored afterwards
        unsafe { std::env::remove_var("SYNTHIA_API_KEY") };
        let state = state_with_auth(vec![]).await;
        assert!(super::validate_ws_token(None, &state));
        assert!(super::validate_ws_token(Some("anything"), &state));
        // Restore original env var
        // SAFETY: test runs on dedicated task, env restored to original state
        unsafe {
            match original {
                Some(v) => std::env::set_var("SYNTHIA_API_KEY", v),
                None => std::env::remove_var("SYNTHIA_API_KEY"),
            }
        }
    }

    /// When an API key is configured, the supplied token must match.
    #[tokio::test]
    #[serial_test::serial]
    async fn validate_ws_token_matches_configured_key() {
        // Save original env var and restore after test
        let original = std::env::var("SYNTHIA_API_KEY").ok();
        // SAFETY: test runs on dedicated task, env restored afterwards
        unsafe { std::env::remove_var("SYNTHIA_API_KEY") };
        let state = state_with_auth(vec!["configured-key".to_string()]).await;
        assert!(super::validate_ws_token(Some("configured-key"), &state));
        assert!(!super::validate_ws_token(Some("wrong"), &state));
        assert!(!super::validate_ws_token(None, &state));
        // Restore original env var
        // SAFETY: test runs on dedicated task, env restored to original state
        unsafe {
            match original {
                Some(v) => std::env::set_var("SYNTHIA_API_KEY", v),
                None => std::env::remove_var("SYNTHIA_API_KEY"),
            }
        }
    }

    /// The env var should take precedence over the configured key.
    #[tokio::test]
    #[serial_test::serial]
    async fn validate_ws_token_env_var_overrides_config() {
        // Save original env var and restore after test
        let original = std::env::var("SYNTHIA_API_KEY").ok();
        // SAFETY: test runs on dedicated task, env restored afterwards
        unsafe { std::env::set_var("SYNTHIA_API_KEY", "env-key") };
        let state = state_with_auth(vec!["config-key".to_string()]).await;
        assert!(super::validate_ws_token(Some("env-key"), &state));
        assert!(!super::validate_ws_token(Some("config-key"), &state));
        // Restore original env var
        // SAFETY: test runs on dedicated task, env restored to original state
        unsafe {
            match original {
                Some(v) => std::env::set_var("SYNTHIA_API_KEY", v),
                None => std::env::remove_var("SYNTHIA_API_KEY"),
            }
        }
    }
}
