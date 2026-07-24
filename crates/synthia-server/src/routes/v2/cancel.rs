//! V2 session cancel endpoint.

use std::sync::Arc;

use axum::{
    Extension,
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};

use super::models::{CancelRequest, CancelResponse};
use crate::{
    api::ApiError,
    middleware::auth::RequestUserId,
    session::controller::SessionOp,
    state::AppState,
};

/// POST /api/v2/sessions/:id/cancel - Cancel the current run.
pub async fn cancel_session(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<RequestUserId>,
    Path(session_id): Path<String>,
    Json(req): Json<CancelRequest>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .session_manager
        .get_for_user(user_id.as_str(), &session_id)
        .await
        .map_err(ApiError::from)?;

    let controller = state
        .get_or_create_session_controller(user_id.as_str(), &session_id)
        .await
        .map_err(|e| ApiError::internal(format!("{:?}", e)))?;

    controller
        .submit(SessionOp::Cancel { reason: req.reason })
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    Ok((
        StatusCode::OK,
        Json(CancelResponse {
            cancelled: true,
            state: format!("{:?}", controller.state()),
        }),
    ))
}
