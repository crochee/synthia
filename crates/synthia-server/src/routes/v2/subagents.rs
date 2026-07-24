//! V2 subagent listing endpoint.
//!
//! GET /api/v2/sessions/{id}/subagents?cursor={cursor}&limit={N}&direction={forward|backward}

use std::sync::Arc;

use axum::{
    Extension,
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};

use super::models::{
    SessionListCursor,
    SessionListQuery,
    SessionSummaryResponse,
};
use crate::{
    api::{ApiError, Cursor, PaginatedResponse, paginate_with_cursor},
    middleware::auth::RequestUserId,
    state::AppState,
};

fn session_state_string(state: synthia_session::types::SessionState) -> String {
    format!("{:?}", state)
}

/// GET /api/v2/sessions/:id/subagents - List child sessions of a session.
pub async fn list_subagents(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<RequestUserId>,
    Path(session_id): Path<String>,
    Query(query): Query<SessionListQuery>,
) -> Result<impl IntoResponse, ApiError> {
    if query.limit == 0 || query.limit > 100 {
        return Err(ApiError::validation_error(vec![
            crate::api::ErrorDetail::new(
                Some("limit"),
                "limit must be between 1 and 100",
                "invalid_limit",
            ),
        ]));
    }

    // Verify the parent session exists and belongs to the caller.
    state
        .session_manager
        .get_for_user(user_id.as_str(), &session_id)
        .await
        .map_err(ApiError::from)?;

    let cursor = query
        .cursor
        .as_deref()
        .map(Cursor::<SessionListCursor>::decode)
        .transpose()?;

    let mut children: Vec<synthia_session::manager::SessionSummary> = state
        .session_manager
        .list_children(user_id.as_str(), &session_id)
        .map_err(|e| ApiError::internal(e.to_string()))?;

    // Sort by updated_at ascending, then id ascending for stable pagination.
    children.sort_by(|a, b| {
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
            children
                .into_iter()
                .filter(|s| {
                    s.updated_at > bound
                        || (s.updated_at == bound && s.id > c.0.id)
                })
                .collect()
        }
        None => children,
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

    let response: PaginatedResponse<SessionSummaryResponse> =
        paginate_with_cursor(
            page,
            next_cursor,
            page_size,
            query.direction,
            &format!("/api/v2/sessions/{}/subagents", session_id),
            |c| Cursor(c.clone()).encode(),
        );

    Ok((StatusCode::OK, Json(response)))
}
