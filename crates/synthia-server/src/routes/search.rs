//! Unified search — `GET /api/v1/search`
//!
//! The thin HTTP projection of the cross-domain
//! [`SearchService`](crate::search::SearchService): one query fans
//! out across every registered engine (tool / mcp / skill / memory
//! / agent / session), the same registry the model-facing `search`
//! tool searches. Capability 2 of the search wrap — the frontend's
//! per-resource search rides this endpoint.
//!
//! # Shape
//!
//! The v1 `List<T>` envelope like every other search surface, with
//! the search owning ordering (best-first, no cursor). `domains`
//! narrows the fan-out as a comma-separated list
//! (`?domains=memory,session`); `q` must be non-empty. Session
//! hits are scoped to the requesting user — the tenancy rule the
//! session store documents — with the same auth-layer fallback the
//! sibling `/sessions/search` uses.

use std::sync::Arc;

use axum::{Extension, Json, extract::State};
use serde::{Deserialize, Serialize};
use synthia::{core::Error, search::Hit};

use crate::{
    api::{AppError, List},
    middleware::auth::RequestUserId,
    state::AppState,
};

/// Query parameters for `GET /api/v1/search`.
#[derive(Debug, Deserialize, validator::Validate)]
pub struct SearchParams {
    /// Free text to look for.
    #[validate(length(min = 1, message = "must not be empty"))]
    pub q: String,
    /// Maximum hits. Default 20, capped at 50 — a discovery call
    /// returns candidates, not a corpus dump.
    #[serde(default)]
    pub limit: Option<usize>,
    /// Comma-separated domain filter
    /// (`tool,mcp,skill,memory,agent,session`). Absent searches
    /// every domain.
    #[serde(default)]
    pub domains: Option<String>,
}

/// Default hit cap when [`SearchParams::limit`] is absent.
const DEFAULT_LIMIT: usize = 20;
/// Ceiling for `limit`.
const MAX_LIMIT: usize = 50;

/// One cross-domain hit in the response.
#[derive(Debug, Serialize)]
pub struct SearchHit {
    pub domain: String,
    pub id: String,
    pub title: String,
    pub score: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
}

impl From<Hit> for SearchHit {
    fn from(h: Hit) -> Self {
        Self {
            domain: h.domain,
            id: h.id,
            title: h.title,
            score: h.score,
            preview: h.preview,
        }
    }
}

/// Parse the comma-separated `domains` filter into labels,
/// dropping empties.
fn parse_domains(raw: Option<&str>) -> Option<Vec<String>> {
    raw.map(|s| {
        s.split(',')
            .map(str::trim)
            .filter(|d| !d.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>()
    })
}

/// GET /api/v1/search — cross-domain search over the deployment's
/// tool / mcp / skill / memory / agent / session catalogs.
pub async fn search_all(
    State(state): State<Arc<AppState>>,
    user: Option<Extension<RequestUserId>>,
    axum::extract::Query(params): axum::extract::Query<SearchParams>,
) -> Result<Json<List<SearchHit>>, AppError> {
    if params.q.trim().is_empty() {
        return Err(AppError::from(Error::invalid_item("query parameter 'q'")));
    }
    let user_id = user
        .map(|Extension(id)| id.0)
        .unwrap_or_else(|| state.default_user_id().to_string());
    let limit = params.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let domains = parse_domains(params.domains.as_deref());
    let hits = state
        .search
        .search(params.q.trim(), limit, domains, Some(&user_id))
        .await;

    // Search-owned ordering: no cursor. `total` is reported only
    // when the cap was not hit, matching the sibling search
    // endpoints' "there may be more" contract.
    let truncated = hits.len() >= limit;
    let total = hits.len() as u64;
    let list = List::new(hits.into_iter().map(SearchHit::from).collect());
    Ok(Json(if truncated {
        list
    } else {
        list.with_total(total)
    }))
}

#[cfg(test)]
mod tests {
    use axum::extract::{Query, State};

    use super::*;

    #[test]
    fn parse_domains_splits_and_drops_empties() {
        assert_eq!(
            parse_domains(Some(" memory , session ,")),
            Some(vec!["memory".to_string(), "session".to_string()])
        );
        assert_eq!(parse_domains(Some("   ")), Some(Vec::new()));
        assert_eq!(parse_domains(None), None);
    }

    #[test]
    fn search_hit_serializes_the_agent_facing_shape() {
        let h = SearchHit::from(Hit {
            item_idx: 0,
            domain: "memory".into(),
            id: "m1".into(),
            title: "PDF preference".into(),
            score: 0.75,
            bm25: 0.5,
            vector: 0.25,
            reasons: vec![],
            preview: Some("prefer plain text".into()),
        });
        let j = serde_json::to_string(&h).unwrap();
        assert!(j.contains("\"domain\":\"memory\""));
        assert!(j.contains("\"preview\":\"prefer plain text\""));
        assert!(!j.contains("bm25"));
    }

    /// A store with two tenants whose transcripts hold distinct
    /// secrets — the case the session domain must not conflate.
    async fn tenant_state() -> (Arc<AppState>, tempfile::TempDir) {
        use synthia::provider::Message;

        let dir = tempfile::tempdir().expect("temp workspace");
        let root = dir.path().join("sessions");
        for (user, text) in [
            ("alice", "the nebula passphrase opens the vault"),
            ("bob", "the quasar passphrase opens the vault"),
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

    async fn search(
        state: &Arc<AppState>,
        user: &str,
        q: &str,
    ) -> Vec<SearchHit> {
        let response = search_all(
            State(Arc::clone(state)),
            Some(Extension(RequestUserId(user.to_string()))),
            Query(SearchParams {
                q: q.to_string(),
                limit: None,
                domains: None,
            }),
        )
        .await
        .expect("the search must succeed");
        response.0.data
    }

    /// The session domain answers with the *requesting user's*
    /// transcripts: alice's query never returns bob's hit, and each
    /// tenant finds their own.
    #[tokio::test]
    async fn the_route_scopes_session_hits_to_the_requesting_user() {
        let (state, _dir) = tenant_state().await;

        let alice_own = search(&state, "alice", "nebula").await;
        assert!(
            alice_own.iter().any(|h| h.domain == "session"),
            "alice must find her own transcript: {alice_own:?}"
        );
        let alice_on_bob = search(&state, "alice", "quasar").await;
        assert!(
            !alice_on_bob.iter().any(|h| h.domain == "session"),
            "alice's search leaked bob's transcript: {alice_on_bob:?}"
        );

        let bob_own = search(&state, "bob", "quasar").await;
        assert!(
            bob_own.iter().any(|h| h.domain == "session"),
            "bob must find his own transcript: {bob_own:?}"
        );
    }

    /// End-to-end through the route: a memory entry stored on the
    /// deployment's tier surfaces as a `memory`-domain hit with its
    /// content preview.
    #[tokio::test]
    async fn the_route_returns_memory_domain_hits() {
        let (state, _dir) = tenant_state().await;
        state
            .memory
            .store(synthia::context::MemoryEntry::now(
                "m-pdf",
                "用户偏好把 pdf 解析为纯文本",
            ))
            .await
            .expect("seed memory");

        let hits = search(&state, "dev", "pdf").await;
        let memory_hit = hits
            .iter()
            .find(|h| h.domain == "memory")
            .expect("memory domain must answer through the route");
        assert_eq!(memory_hit.id, "m-pdf");
        assert!(memory_hit.preview.is_some());
    }
}
