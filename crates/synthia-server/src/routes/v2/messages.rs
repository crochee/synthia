//! V2 message listing endpoint with cursor-based pagination by sequence number.
//!
//! GET /api/v2/sessions/{id}/messages?cursor={cursor}&limit={N}&direction={forward|backward}

use std::sync::Arc;

use axum::{
    Extension,
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};

use super::models::{MessageCursor, MessageItem, MessagesQuery};
use crate::{
    api::{ApiError, Cursor, PaginatedResponse, paginate_with_cursor},
    middleware::auth::RequestUserId,
    state::AppState,
};

/// GET /api/v2/sessions/:id/messages - List messages for a session.
pub async fn list_messages(
    State(state): State<Arc<AppState>>,
    Extension(user_id): Extension<RequestUserId>,
    Path(session_id): Path<String>,
    Query(query): Query<MessagesQuery>,
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

    state
        .session_manager
        .get_for_user(user_id.as_str(), &session_id)
        .await
        .map_err(ApiError::from)?;

    let cursor = query
        .cursor
        .as_deref()
        .map(Cursor::<MessageCursor>::decode)
        .transpose()?;

    let messages: Vec<synthia_provider::Message> = state
        .session_manager
        .store()
        .load_messages_all(user_id.as_str(), &session_id)
        .map_err(|e| ApiError::internal(e.to_string()))?;

    let mut items: Vec<MessageItem> = messages
        .into_iter()
        .enumerate()
        .map(|(idx, m)| MessageItem {
            seq: (idx + 1) as u64,
            role: format!("{:?}", m.role),
            content: match m.content {
                synthia_provider::Content::Single(part) => {
                    part.text().unwrap_or("").to_string()
                }
                synthia_provider::Content::Multi(parts) => parts
                    .iter()
                    .filter_map(|p| p.text())
                    .collect::<Vec<_>>()
                    .join(" "),
            },
        })
        .collect();

    items.sort_by_key(|a| a.seq);

    let filtered: Vec<MessageItem> = match cursor {
        Some(ref c) => items.into_iter().filter(|m| m.seq > c.0.seq).collect(),
        None => items,
    };

    let page_size = query.limit;
    let page: Vec<MessageItem> =
        filtered.into_iter().take(page_size + 1).collect();
    let next_cursor = if page.len() > page_size {
        page.last().map(|last| MessageCursor { seq: last.seq })
    } else {
        None
    };

    let response: PaginatedResponse<MessageItem> = paginate_with_cursor(
        page,
        next_cursor,
        page_size,
        query.direction,
        &format!("/api/v2/sessions/{}/messages", session_id),
        |c| Cursor(c.clone()).encode(),
    );

    Ok((StatusCode::OK, Json(response)))
}
