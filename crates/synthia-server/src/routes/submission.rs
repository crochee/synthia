//! `POST /submission` — accepts a `Submission` envelope, dispatches via
//! `SessionManager`, and returns immediately (202 Accepted).
//!
//! Round 6 of `synthia-session-v2.md` — wire protocol over HTTP.

use std::sync::Arc;

use axum::{
    Extension,
    Json,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use synthia_protocol::{Op, Submission};

use crate::{middleware::auth::RequestUserId, state::AppState};

/// Envelope that the wire endpoint accepts.
///
/// The HTTP body must include `user_id`, `session_id`, and the `submission`
/// payload. The `submission` field deserializes into
/// `synthia_protocol::Submission` (frozen at R1) and is the only field that
/// participates in protocol versioning.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SubmissionEnvelope {
    /// User owning the submission. Resolved by auth middleware for
    /// the `RequestUserId` extension, but echoed back in the body for
    /// traceability.
    pub user_id: String,
    /// Target session identifier (non-empty).
    pub session_id: String,
    /// Wire protocol submission payload.
    pub submission: Submission,
}

/// Response body for `POST /submission`.
#[derive(Debug, Clone, Serialize)]
pub struct SubmissionAck {
    pub status: &'static str,
    pub submission_id: String,
    pub session_id: String,
    pub user_id: String,
}

/// `POST /submission`
///
/// Validates the envelope, ensures the session exists (creating it if
/// absent), and returns `202 Accepted`. The actual agent loop runs
/// asynchronously; clients read events from `GET /ws`.
pub async fn post_submission(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<RequestUserId>,
    Json(envelope): Json<SubmissionEnvelope>,
) -> impl IntoResponse {
    // User id from the auth middleware is authoritative; the body field
    // is informational.
    let _ = envelope.user_id;

    let session_id = envelope.session_id.trim().to_string();
    if session_id.is_empty() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({
                "status": "error",
                "error": {
                    "code": "missing_session_id",
                    "message": "session_id must be a non-empty string",
                }
            })),
        )
            .into_response();
    }

    let user_id_str = user_id.0.clone();
    if user_id_str.is_empty() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "status": "error",
                "error": {
                    "code": "missing_user_id",
                    "message": "user_id must be a non-empty string",
                }
            })),
        )
            .into_response();
    }

    // Ensure the session exists in the session manager. This is a cheap
    // upsert; we do NOT block on agent execution.
    let _ = state
        .session_manager
        .create_with_user(session_id.clone(), user_id_str.clone())
        .await;

    tracing::info!(
        submission_id = %envelope.submission.id,
        session_id = %session_id,
        op = ?std::mem::discriminant(&envelope.submission.op),
        user_id = %user_id_str,
        "accepted submission"
    );

    // Spawn the dispatch task but return 202 immediately.
    let _state = state.clone();
    let _submission = envelope.submission.clone();
    tokio::spawn(async move {
        // Round 6 intentionally does NOT block the HTTP handler on agent
        // execution. Round 7+ will route these submissions through the
        // session controller. For now we just emit a debug trace so
        // operators can observe receipt.
        tracing::debug!(op = ?discriminant_str(&_submission.op), "dispatch stub");
    });

    (
        StatusCode::ACCEPTED,
        Json(SubmissionAck {
            status: "accepted",
            submission_id: envelope.submission.id.to_string(),
            session_id,
            user_id: user_id_str,
        }),
    )
        .into_response()
}

fn discriminant_str(op: &Op) -> &'static str {
    match op {
        Op::Interrupt { .. } => "interrupt",
        Op::Compact { .. } => "compact",
        Op::UserInput { .. } => "user_input",
        Op::ThreadRollback { .. } => "thread_rollback",
        Op::ApprovalResponse { .. } => "approval_response",
        Op::RefreshTools => "refresh_tools",
        Op::Resubmit { .. } => "resubmit",
        Op::UpdateModel { .. } => "update_model",
        Op::UpdateThinkingLevel { .. } => "update_thinking_level",
        Op::SwitchSession { .. } => "switch_session",
        Op::ForkSession { .. } => "fork_session",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, sync::Arc};

    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        routing::post,
    };
    use http_body_util::BodyExt;
    use synthia_protocol::{InputItem, Op, Submission, SubmissionId};
    use synthia_session::manager::SessionManager;
    use tower::ServiceExt;

    use super::*;

    /// Test-only middleware that injects a `RequestUserId` so the handler
    /// can be exercised without the full auth stack.
    async fn inject_user_id(
        mut req: axum::http::Request<axum::body::Body>,
        next: axum::middleware::Next,
    ) -> axum::http::Response<axum::body::Body> {
        req.extensions_mut()
            .insert(RequestUserId("test-user".to_string()));
        next.run(req).await
    }

    fn build_test_app() -> Router {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace: PathBuf = temp.path().to_path_buf();
        let session_manager =
            SessionManager::new(workspace.join(".synthia").join("sessions"));
        let state = Arc::new(AppState::for_test(session_manager, workspace));
        Router::new()
            .route("/submission", post(post_submission))
            .layer(axum::middleware::from_fn(inject_user_id))
            .with_state(state)
    }

    fn sample_submission() -> Submission {
        Submission {
            id: SubmissionId::new(),
            op: Op::UserInput {
                items: vec![InputItem::Text {
                    text: "hello".to_string(),
                }],
                final_output_json_schema: None,
                additional_context: None,
            },
            client_user_message_id: None,
            trace: None,
        }
    }

    #[tokio::test]
    async fn post_submission_returns_202_on_happy_path() {
        let app = build_test_app();
        let body = json!({
            "user_id": "alice",
            "session_id": "sess-1",
            "submission": sample_submission(),
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/submission")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body_bytes =
            response.into_body().collect().await.unwrap().to_bytes();
        let parsed: serde_json::Value =
            serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(parsed["status"], "accepted");
        assert_eq!(parsed["session_id"], "sess-1");
        // user_id in the response comes from the auth middleware
        // extension (authoritative), not from the body echo.
        assert_eq!(parsed["user_id"], "test-user");
        assert!(parsed["submission_id"].is_string());
    }

    #[tokio::test]
    async fn post_submission_returns_400_on_bad_json() {
        let app = build_test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/submission")
                    .header("content-type", "application/json")
                    .body(Body::from("{not json"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn post_submission_returns_422_on_missing_session_id() {
        let app = build_test_app();
        let body = json!({
            "user_id": "alice",
            "session_id": "",
            "submission": sample_submission(),
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/submission")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body_bytes =
            response.into_body().collect().await.unwrap().to_bytes();
        let parsed: serde_json::Value =
            serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(parsed["error"]["code"], "missing_session_id");
    }
}
