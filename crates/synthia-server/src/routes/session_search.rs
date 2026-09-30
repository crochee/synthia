//! Session search — `GET /api/v1/sessions/search`
//!
//! Finds an earlier conversation by its text. The search itself is a
//! library seam ([`synthia::session::SessionSearch`]); this module is the
//! thin HTTP projection of it, so the same index a Rust consumer builds
//! serves the UI.
//!
//! # Shape
//!
//! The response is the v1 `List<T>` envelope, like every other search
//! surface (`/api/v1/search`), but **the search owns ordering**:
//! results come back best-first and `cursor` is not meaningful, so the
//! envelope carries the hits with `has_more` from the limit and no
//! `next_cursor`. `limit` caps the hits; `session_id` narrows to one
//! conversation; and the search is **scoped to the requesting user**, so
//! one tenant's query cannot be answered with another's transcript.

use std::sync::Arc;

use axum::{Extension, Json, extract::State};
use serde::{Deserialize, Serialize};
use synthia::{
    core::Error,
    session::{SearchQuery, SessionHit},
};

use crate::{
    api::{AppError, List},
    middleware::auth::RequestUserId,
    state::AppState,
};

/// Query parameters for `GET /api/v1/sessions/search`.
#[derive(Debug, Deserialize, validator::Validate)]
pub struct SessionSearchParams {
    /// Free text to look for.
    #[validate(length(min = 1, message = "must not be empty"))]
    pub q: String,
    /// Maximum hits. Defaults to the library's
    /// [`synthia::session::DEFAULT_LIMIT`].
    #[serde(default)]
    pub limit: Option<usize>,
    /// Restrict the search to one session.
    #[serde(default)]
    pub session_id: Option<String>,
}

/// One session in the response.
#[derive(Debug, Serialize)]
pub struct SessionSearchResult {
    pub session_id: String,
    pub score: f32,
    pub matched_entries: usize,
    /// Best entry of the session, when one matched.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top: Option<SessionSearchTopHit>,
}

/// The best entry of a [`SessionSearchResult`].
#[derive(Debug, Serialize)]
pub struct SessionSearchTopHit {
    pub seq: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    pub snippet: String,
    pub score: f32,
}

impl From<SessionHit> for SessionSearchResult {
    fn from(hit: SessionHit) -> Self {
        Self {
            session_id: hit.session_id,
            score: hit.score,
            matched_entries: hit.matched_entries,
            top: hit.top.map(|top| SessionSearchTopHit {
                seq: top.seq,
                timestamp: top.timestamp,
                snippet: top.snippet,
                score: top.score,
            }),
        }
    }
}

/// GET /api/v1/sessions/search — search the session store's text.
///
/// The search is scoped to the requesting user: the store holds every
/// tenant's transcripts, so an unscoped query would answer one tenant with
/// another's history. The scope comes from the `RequestUserId` the auth
/// layer resolved for this request, falling back to the server default
/// only when no auth layer ran.
pub async fn search_sessions(
    State(state): State<Arc<AppState>>,
    user: Option<Extension<RequestUserId>>,
    axum::extract::Query(params): axum::extract::Query<SessionSearchParams>,
) -> Result<Json<List<SessionSearchResult>>, AppError> {
    if params.q.trim().is_empty() {
        return Err(AppError::from(Error::invalid_item("query parameter 'q'")));
    }
    let user_id = user
        .map(|Extension(id)| id.0)
        .unwrap_or_else(|| state.default_user_id().to_string());
    let mut query = SearchQuery::new(params.q).in_user(user_id);
    if let Some(limit) = params.limit {
        query = query.with_limit(limit);
    }
    if let Some(session_id) = params.session_id {
        query = query.in_session(session_id);
    }
    let limit = query.limit.unwrap_or(synthia::session::DEFAULT_LIMIT);
    // No `sync()` here: `search_sessions` refreshes the index itself, and
    // it does so scoped to the query's user — an unscoped sync at this
    // point would walk every tenant's logs to answer one tenant's search.
    let hits = state
        .session_search
        .search_sessions(&query)
        .await
        .map_err(|e| AppError::from(Error::internal(format!("search: {e}"))))?;

    // The search owns ordering, so there is no cursor to hand back: the
    // envelope is the shared one with `data` and, when the cap was hit, a
    // `total` that tells the caller "there may be more".
    let truncated = hits.len() >= limit;
    let total = if truncated {
        None
    } else {
        Some(hits.len() as u64)
    };
    let list =
        List::new(hits.into_iter().map(SessionSearchResult::from).collect());
    Ok(Json(match total {
        Some(total) => list.with_total(total),
        None => list,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A store with two tenants who picked the same session id — the case
    /// an index keyed by that id alone cannot tell apart.
    async fn tenant_state() -> (Arc<AppState>, tempfile::TempDir) {
        use synthia::provider::Message;

        let dir = tempfile::tempdir().expect("temp workspace");
        let root = dir.path().join("sessions");
        for (user, text) in [
            ("alice", "my secret is the nebula passphrase"),
            ("bob", "my secret is the quasar passphrase"),
        ] {
            let session = root.join(user).join("main");
            std::fs::create_dir_all(&session).expect("session dir");
            let row = serde_json::json!({
                "type": "user_message",
                "seq": 1,
                "ts": "2026-09-12T09:00:00Z",
                "data": Message::user(text),
            });
            std::fs::write(session.join("events.jsonl"), format!("{row}\n"))
                .expect("transcript");
        }
        let registry = synthia::session::manager::SessionRegistry::new(root);
        let state =
            AppState::for_test(registry, dir.path().to_path_buf()).await;
        (Arc::new(state), dir)
    }

    /// Drive the handler with the `RequestUserId` the auth layer would
    /// have resolved (`None` = no auth layer ran).
    async fn search(
        state: &Arc<AppState>,
        user: Option<RequestUserId>,
        q: &str,
    ) -> Vec<SessionSearchResult> {
        let response = search_sessions(
            State(Arc::clone(state)),
            user.map(Extension),
            axum::extract::Query(SessionSearchParams {
                q: q.to_string(),
                limit: None,
                session_id: None,
            }),
        )
        .await
        .expect("the search must succeed");
        response.0.data
    }

    /// The scope is the *request's* user, not the server default: a
    /// request authenticated as alice is answered from alice's directory
    /// even though this deployment's default user is someone else — and
    /// never from bob's, whose session id is the same string.
    #[tokio::test]
    async fn the_route_searches_the_requesting_users_sessions_only() {
        let (state, _dir) = tenant_state().await;

        let leaked =
            search(&state, Some(RequestUserId("alice".to_string())), "quasar")
                .await;
        assert!(
            leaked.is_empty(),
            "alice's search was answered with bob's transcript: {leaked:?}"
        );

        let own =
            search(&state, Some(RequestUserId("alice".to_string())), "nebula")
                .await;
        assert_eq!(own.len(), 1, "{own:?}");
        assert_eq!(own[0].session_id, "main");

        let bobs =
            search(&state, Some(RequestUserId("bob".to_string())), "quasar")
                .await;
        assert_eq!(bobs.len(), 1, "{bobs:?}");
        assert_eq!(bobs[0].session_id, "main");

        // No extension (no auth layer): the handler falls back to the
        // server default, which owns nothing in this store — the fallback
        // must not hand anyone another tenant's hits.
        assert!(
            search(&state, None, "nebula").await.is_empty()
                && search(&state, None, "quasar").await.is_empty(),
            "the default user must not inherit a tenant's hits"
        );
    }

    #[test]
    fn a_hit_projects_its_top_entry() {
        let hit = SessionHit {
            session_id: "s1".to_string(),
            score: 1.5,
            matched_entries: 2,
            top: Some(synthia::session::EntryHit {
                session_id: "s1".to_string(),
                seq: 7,
                timestamp: Some("2026-09-12T09:00:00Z".to_string()),
                snippet: "…tokens expire…".to_string(),
                score: 1.5,
            }),
        };
        let json =
            serde_json::to_value(SessionSearchResult::from(hit)).unwrap();
        assert_eq!(json["session_id"], "s1");
        assert_eq!(json["matched_entries"], 2);
        assert_eq!(json["top"]["seq"], 7);
        assert_eq!(json["top"]["timestamp"], "2026-09-12T09:00:00Z");
    }

    #[test]
    fn a_hit_without_a_top_entry_omits_the_field() {
        let hit = SessionHit {
            session_id: "s2".to_string(),
            score: 0.5,
            matched_entries: 1,
            top: None,
        };
        let json =
            serde_json::to_value(SessionSearchResult::from(hit)).unwrap();
        assert!(json.get("top").is_none(), "{json}");
    }
}
