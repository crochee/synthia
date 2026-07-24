//! V2 steering input endpoint.

use std::sync::Arc;

use axum::{
    Extension,
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};

use super::models::{SteeringAcceptedResponse, SteeringRequest};
use crate::{
    api::{ApiError, validate_content_not_empty, validate_priority},
    middleware::auth::RequestUserId,
    session::controller::SessionOp,
    state::AppState,
};

/// POST /api/v2/sessions/:id/steering - Submit a steering message.
pub async fn create_steering(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<RequestUserId>,
    Path(session_id): Path<String>,
    Json(req): Json<SteeringRequest>,
) -> Result<impl IntoResponse, ApiError> {
    validate_content_not_empty(&req.content)?;
    let priority = req.priority.unwrap_or(255);
    validate_priority(priority)?;

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
        .submit(SessionOp::Steer {
            content: req.content,
            priority,
        })
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    Ok((
        StatusCode::ACCEPTED,
        Json(SteeringAcceptedResponse {
            admitted: true,
            state: format!("{:?}", controller.state()),
        }),
    ))
}
