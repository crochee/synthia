//! V2 session lifecycle endpoints.

use std::sync::Arc;

use axum::{
    Extension,
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};

use super::models::{
    CreateSessionRequest,
    SessionListCursor,
    SessionListQuery,
    SessionResponse,
    SessionSummaryResponse,
};
use crate::{
    api::{
        ApiError,
        Cursor,
        PaginatedResponse,
        json_data,
        paginate_with_cursor,
    },
    middleware::auth::RequestUserId,
    state::AppState,
};

fn session_state_string(state: synthia_session::types::SessionState) -> String {
    format!("{:?}", state)
}

/// POST /api/v2/sessions - Create a new session.
pub async fn create_session_v2(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<RequestUserId>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Response, ApiError> {
    let session_id = synthia_core::generate_session_id();
    let mut session = state
        .session_manager
        .create_with_user(session_id.clone(), user_id.as_str().to_string())
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    if let Some(model) = req.model {
        session.config.model = model;
    }

    let response = SessionResponse {
        id: session.id.clone(),
        state: session_state_string(session.state),
        model: session.config.model.clone(),
        title: req.title,
        parent_id: session.parent_id.clone(),
        max_iterations: req.max_iterations,
        created_at: session.created_at.to_rfc3339(),
        updated_at: session.updated_at.to_rfc3339(),
    };

    let location = format!("/api/v2/sessions/{}", session.id);
    Ok((
        StatusCode::CREATED,
        [("Location", location)],
        json_data(response),
    )
        .into_response())
}

/// GET /api/v2/sessions - List sessions for the current user.
pub async fn list_sessions_v2(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<RequestUserId>,
    Query(query): Query<SessionListQuery>,
) -> Result<Json<PaginatedResponse<SessionSummaryResponse>>, ApiError> {
    if query.limit == 0 || query.limit > 100 {
        return Err(ApiError::validation_error(vec![
            crate::api::ErrorDetail::new(
                Some("limit"),
                "limit must be between 1 and 100",
                "invalid_limit",
            ),
        ]));
    }

    let cursor = query
        .cursor
        .as_deref()
        .map(Cursor::<SessionListCursor>::decode)
        .transpose()?;

    let mut sessions: Vec<synthia_session::manager::SessionSummary> =
        state.session_manager.list_for_user(user_id.as_str()).await;

    // Sort by updated_at ascending, then id ascending for stable pagination.
    sessions.sort_by(|a, b| {
        a.updated_at
            .cmp(&b.updated_at)
            .then_with(|| a.id.cmp(&b.id))
    });

    // Apply cursor filter for forward pagination.
    let filtered: Vec<synthia_session::manager::SessionSummary> = match cursor {
        Some(ref c) => {
            let bound = chrono::DateTime::parse_from_rfc3339(&c.0.updated_at)
                .map_err(|_| ApiError::invalid_cursor())?
                .with_timezone(&chrono::Utc);
            sessions
                .into_iter()
                .filter(|s| {
                    s.updated_at > bound
                        || (s.updated_at == bound && s.id > c.0.id)
                })
                .collect()
        }
        None => sessions,
    };

    let page_size = query.limit;
    let page: Vec<SessionSummaryResponse> = filtered
        .into_iter()
        .take(page_size + 1)
        .map(|s| SessionSummaryResponse {
            id: s.id.clone(),
            state: session_state_string(s.state),
            title: s.title,
            parent_id: s.parent_id.clone(),
            updated_at: s.updated_at.to_rfc3339(),
        })
        .collect();

    let next_cursor = if page.len() > page_size {
        page.last().map(|item| SessionListCursor {
            updated_at: item.updated_at.clone(),
            id: item.id.clone(),
        })
    } else {
        None
    };

    let response = paginate_with_cursor(
        page,
        next_cursor,
        page_size,
        query.direction,
        "/api/v2/sessions",
        |c| Cursor(c.clone()).encode(),
    );

    Ok(Json(response))
}

/// GET /api/v2/sessions/:id - Get session detail.
pub async fn get_session_detail(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<RequestUserId>,
    Path(session_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let session = state
        .session_manager
        .get_for_user(user_id.as_str(), &session_id)
        .await
        .map_err(ApiError::from)?;

    Ok(json_data(SessionResponse {
        id: session.id,
        state: session_state_string(session.state),
        model: session.config.model,
        title: None,
        parent_id: session.parent_id,
        max_iterations: None,
        created_at: session.created_at.to_rfc3339(),
        updated_at: session.updated_at.to_rfc3339(),
    }))
}

/// DELETE /api/v2/sessions/:id - Delete a session.
pub async fn delete_session_v2(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<RequestUserId>,
    Path(session_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state
        .session_manager
        .delete_for_user(user_id.as_str(), &session_id)
        .await
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}
