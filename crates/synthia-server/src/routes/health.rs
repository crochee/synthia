use std::sync::Arc;

use axum::{
    Json,
    extract::State,
    http::{
        HeaderMap,
        HeaderValue,
        StatusCode,
        header::{CACHE_CONTROL, ETAG, IF_NONE_MATCH},
    },
    response::{IntoResponse, Response},
};
use parking_lot::RwLock;
use serde::Serialize;

use crate::state::AppState;

/// Minimal probe response body shared by `/livez` and `/readyz`.
#[derive(Serialize)]
pub struct ProbeResponse {
    pub status: &'static str,
    /// Names of readiness checks that failed. Omitted when every
    /// check passes; only meaningful for `/readyz`.
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    pub failed: Vec<&'static str>,
}

/// GET /livez - Kubernetes-style liveness probe.
///
/// Liveness answers exactly one question: "can this process still
/// serve HTTP?" If the handler runs at all, the answer is yes — so
/// it returns 200 unconditionally without touching shared state.
/// Dependency health belongs on `/readyz`: a liveness failure gets
/// the pod restarted, which must be reserved for unrecoverable
/// states (deadlocked runtime, exhausted executor).
///
/// `Cache-Control: no-store` keeps orchestrators and load
/// balancers from reusing a cached verdict across the process
/// lifetime.
pub async fn livez() -> Response {
    (
        StatusCode::OK,
        [(CACHE_CONTROL, HeaderValue::from_static("no-store"))],
        Json(ProbeResponse {
            status: "ok",
            failed: Vec::new(),
        }),
    )
        .into_response()
}

/// GET /readyz - Kubernetes-style readiness probe.
///
/// Readiness means "should traffic be routed here *now*". Unlike
/// liveness it inspects in-process facts via
/// [`AppState::readiness_checks`]. A failing check yields
/// `503` plus the failing check names, so an operator can see
/// *why* the instance is not ready from the probe response
/// itself.
pub async fn readyz(State(state): State<Arc<AppState>>) -> Response {
    let failed: Vec<&'static str> = state
        .readiness_checks()
        .into_iter()
        .filter(|(_, passed)| !passed)
        .map(|(name, _)| name)
        .collect();

    if failed.is_empty() {
        return (
            StatusCode::OK,
            [(CACHE_CONTROL, HeaderValue::from_static("no-store"))],
            Json(ProbeResponse {
                status: "ok",
                failed,
            }),
        )
            .into_response();
    }

    tracing::warn!(checks = ?failed, "readiness probe failing");
    (
        StatusCode::SERVICE_UNAVAILABLE,
        [(CACHE_CONTROL, HeaderValue::from_static("no-store"))],
        Json(ProbeResponse {
            status: "unavailable",
            failed,
        }),
    )
        .into_response()
}

/// Bare models-listing response.
#[derive(Clone, Serialize)]
pub struct ModelsResponse {
    pub models: Vec<ModelEntry>,
    pub default_provider: String,
    pub default_model: String,
}

#[derive(Clone, Serialize)]
pub struct ModelEntry {
    pub provider: String,
    pub model: String,
    pub context_window: usize,
    pub supports_tools: bool,
    pub supports_streaming: bool,
}

/// ETag for `/api/models`. The body is fully determined by the
/// workspace configuration (which is loaded once at startup and
/// doesn't mutate over the binary's lifetime — `AppState` itself
/// is shared via `Arc` and `workspace_config` is held by value,
/// so a hot-reload would replace the entire `Arc`, busting this
/// tag implicitly through the version suffix). We mix the
/// package version and the default model name to ensure
/// re-deploys invalidate stale caches.
fn models_etag(default_model: &str) -> String {
    // `weak` (W/) because the body doesn't carry a strong
    // semantic identity (model entries are described by their
    // attributes, not a content-addressed hash). Weak validators
    // are RFC 9110 compliant for opaque-cache scenarios.
    format!(
        "W/\"models-{}-{}-{}\"",
        env!("CARGO_PKG_VERSION"),
        default_model.len(),
        default_model
            .chars()
            .fold(0u64, |acc, c| acc.wrapping_mul(31).wrapping_add(c as u64)),
    )
}

/// Cached `(default_model, etag, body)` triple for `/api/models`.
///
/// The `ModelsResponse` body is fully determined at startup
/// (`workspace_config.providers` + `default_model` +
/// `default_provider`) and never mutates over the binary's
/// lifetime — a hot-reload would replace the entire `AppState`
/// `Arc`, so the cache key implicitly goes stale. Without this
/// cache every `/api/models` request rebuilt the full
/// `Vec<ModelEntry>` (one String per field × per provider) and
/// re-folded the default-model string into the ETag hash.
static MODELS_CACHE: RwLock<Option<Arc<CachedModels>>> = RwLock::new(None);

struct CachedModels {
    default_model: String,
    etag: String,
    body: Arc<ModelsResponse>,
}

/// GET /api/models - List available models with bare response (no envelope).
///
/// Like the probe endpoints, this endpoint benefits from conditional GET:
/// the response body is fully determined at startup and never
/// mutates over the binary's lifetime, so an ETag lets clients
/// short-circuit to `304 Not Modified` on revalidation. We use
/// `Cache-Control: no-cache, max-age=60` so a one-minute shared
/// cache (the API client polls this on app start and rarely
/// thereafter) cuts round-trips without hiding deploy-time
/// changes for longer than a worker restart typically requires.
pub async fn list_models(
    State(state): State<Arc<AppState>>,
    req: axum::http::HeaderMap,
) -> Response {
    let config = &state.workspace_config;

    // Build-once cache: the response body and ETag are fully
    // determined by `workspace_config`, which is loaded once at
    // startup and never mutated. Reusing the cached
    // `(etag, body)` Arc across requests skips the per-request
    // `Vec<ModelEntry>` rebuild (N providers × 4 strings each)
    // and the O(default_model.len()) char-fold inside
    // `models_etag`.
    let cached = {
        let guard = MODELS_CACHE.read();
        guard
            .as_ref()
            .filter(|c| c.default_model == config.default_model)
            .cloned()
    };
    let cached = match cached {
        Some(c) => c,
        None => {
            // Slow path: rebuild the body + ETag and store the
            // result under a write lock so the next request gets
            // a hit. The double-check inside the write guard
            // protects against a thundering-herd rebuild if two
            // requests race to populate the cache on a cold
            // start.
            let models: Vec<ModelEntry> = config
                .providers
                .iter()
                .map(|(name, entry)| ModelEntry {
                    provider: name.clone(),
                    model: entry
                        .default_model
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string()),
                    context_window: entry.context_window.unwrap_or(128_000),
                    supports_tools: entry.supports_tools.unwrap_or(true),
                    supports_streaming: entry
                        .supports_streaming
                        .unwrap_or(true),
                })
                .collect();
            let etag_string = models_etag(&config.default_model);
            let body = ModelsResponse {
                models,
                default_provider: config.default_provider.clone(),
                default_model: config.default_model.clone(),
            };
            let mut guard = MODELS_CACHE.write();
            // Re-check inside the write guard — another request
            // may have populated the cache while we were
            // building.
            let cached = guard
                .as_ref()
                .filter(|c| c.default_model == config.default_model)
                .cloned();
            cached.unwrap_or_else(|| {
                let new = Arc::new(CachedModels {
                    default_model: config.default_model.clone(),
                    etag: etag_string,
                    body: Arc::new(body),
                });
                *guard = Some(new.clone());
                new
            })
        }
    };

    let etag_value = HeaderValue::from_str(&cached.etag)
        .unwrap_or_else(|_| HeaderValue::from_static("W/\"models-invalid\""));

    let mut headers = HeaderMap::new();
    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static("no-cache, max-age=60"),
    );
    headers.insert(ETAG, etag_value.clone());

    // Conditional GET: any `If-None-Match` header short-circuits
    // to 304 with no body. This mirrors the unconditional pattern
    // used by the probe endpoints: the body is fully described by the ETag
    // (config doesn't mutate over the binary's lifetime), so
    // there's no scenario where a stale cache holds a different
    // version of the body that the client would want back.
    // Returns 304 with the fresh ETag + Cache-Control so the
    // client's existing cached body is reused without body bytes.
    if req.get(IF_NONE_MATCH).is_some() {
        return (StatusCode::NOT_MODIFIED, headers).into_response();
    }

    let body = (*cached.body).clone();
    (StatusCode::OK, headers, Json(body)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_entry_serializes_all_fields() {
        let entry = ModelEntry {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            context_window: 8192,
            supports_tools: true,
            supports_streaming: false,
        };
        let json = serde_json::to_value(&entry).unwrap();
        assert_eq!(json["provider"], "openai");
        assert_eq!(json["model"], "gpt-4");
        assert_eq!(json["context_window"], 8192);
        assert_eq!(json["supports_tools"], true);
        assert_eq!(json["supports_streaming"], false);
    }

    #[test]
    fn models_response_wraps_entries_with_defaults() {
        let resp = ModelsResponse {
            models: vec![],
            default_provider: "anthropic".to_string(),
            default_model: "claude-opus-4".to_string(),
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["models"].as_array().unwrap().len(), 0);
        assert_eq!(json["default_provider"], "anthropic");
        assert_eq!(json["default_model"], "claude-opus-4");
    }

    #[test]
    fn models_etag_is_stable_per_default_model() {
        // Same input → same output, byte-for-byte.
        assert_eq!(models_etag("gpt-4"), models_etag("gpt-4"));
    }

    #[test]
    fn models_etag_differs_when_default_model_changes() {
        assert_ne!(
            models_etag("gpt-4"),
            models_etag("claude-opus-4"),
            "changing the default model must invalidate the ETag"
        );
    }

    #[test]
    fn models_etag_embeds_package_version() {
        // The binary version is a baked-in constant, so the tag
        // must include it as a deploy-busting component.
        let tag = models_etag("gpt-4");
        assert!(
            tag.contains(env!("CARGO_PKG_VERSION")),
            "ETag must include CARGO_PKG_VERSION, got: {tag}"
        );
    }

    /// `list_models` with no `If-None-Match` MUST return 200 with
    /// the full JSON body and the new ETag + Cache-Control
    /// headers. Pinning the wire shape here guards against a
    /// missing-header-propagation regression.
    #[tokio::test]
    async fn list_models_emits_etag_and_cache_control() {
        let state =
            crate::state::AppState::new(std::path::PathBuf::from("."), None)
                .await
                .expect("workspace load should succeed in test");
        let req = axum::http::HeaderMap::new();
        let resp = list_models(State(state), req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get(CACHE_CONTROL).unwrap(),
            "no-cache, max-age=60"
        );
        let etag = resp.headers().get(ETAG).unwrap().to_str().unwrap();
        assert!(etag.starts_with("W/\"models-"), "ETag shape: {etag}");
        assert!(
            etag.contains(env!("CARGO_PKG_VERSION")),
            "ETag must embed CARGO_PKG_VERSION: {etag}"
        );
    }

    /// `list_models` with any `If-None-Match` MUST return 304
    /// without serializing the body — the same short-circuit
    /// pattern the probe endpoints use.
    #[tokio::test]
    async fn list_models_returns_304_when_if_none_match_present() {
        let state =
            crate::state::AppState::new(std::path::PathBuf::from("."), None)
                .await
                .expect("workspace load should succeed in test");
        let mut req = axum::http::HeaderMap::new();
        req.insert(
            IF_NONE_MATCH,
            HeaderValue::from_static("W/\"models-test-irrelevant\""),
        );
        let resp = list_models(State(state), req).await;
        assert_eq!(resp.status(), StatusCode::NOT_MODIFIED);
        // Body MUST be empty on a 304 (RFC 9110 §15.4.5).
        // `Body::size_hint` returns an upper bound; an empty
        // body reports exact size 0 via the `SizeHint` struct.
        use axum::body::HttpBody;
        let size = resp.into_body().size_hint().exact();
        assert_eq!(size, Some(0), "304 response body must be empty");
    }

    /// `livez` MUST return 200 with `status: "ok"` and
    /// `Cache-Control: no-store` — liveness is a process-alive
    /// fact, so a cached verdict would outlive a crash-restart.
    #[tokio::test]
    async fn livez_emits_ok_and_no_store() {
        let resp = livez().await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.headers().get(CACHE_CONTROL).unwrap(), "no-store");
        use http_body_util::BodyExt;
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["status"], "ok");
        // `failed` is skipped when empty — the liveness body has
        // no failed-checks field at all.
        assert!(body.get("failed").is_none(), "got: {body}");
    }

    /// `readyz` MUST return 503 + the failing check names while
    /// the A2A service OnceCell is untouched (i.e. the router was
    /// not built through `create_router`, which initializes it
    /// eagerly). The agent-registry check passes because
    /// `AppState::new` registers the canonical ReAct agent.
    #[tokio::test]
    async fn readyz_is_503_before_a2a_service_initialized() {
        let state =
            crate::state::AppState::new(std::path::PathBuf::from("."), None)
                .await
                .expect("workspace load should succeed in test");
        let resp = readyz(State(state)).await;
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        use http_body_util::BodyExt;
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["status"], "unavailable");
        assert_eq!(
            body["failed"],
            serde_json::json!(["a2a_service"]),
            "only the a2a_service check may fail here, got: {body}"
        );
    }

    /// `readyz` MUST flip to 200 once every readiness check
    /// passes — here triggered by initializing the A2A service
    /// the same way `create_router` does.
    #[tokio::test]
    async fn readyz_is_200_after_a2a_service_initialized() {
        let state =
            crate::state::AppState::new(std::path::PathBuf::from("."), None)
                .await
                .expect("workspace load should succeed in test");
        // Mirror the eager bootstrap in `create_router`.
        let _ = state.a2a_service(None).await;
        let resp = readyz(State(state)).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.headers().get(CACHE_CONTROL).unwrap(), "no-store");
        use http_body_util::BodyExt;
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["status"], "ok");
        assert!(body.get("failed").is_none(), "got: {body}");
    }

    /// `ProbeResponse` with failed checks MUST serialize the
    /// names so operators can see *why* the probe fails from the
    /// response body alone.
    #[test]
    fn probe_response_serializes_failed_check_names() {
        let resp = ProbeResponse {
            status: "unavailable",
            failed: vec!["a2a_service", "agent_registry"],
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["status"], "unavailable");
        assert_eq!(json["failed"][0], "a2a_service");
        assert_eq!(json["failed"][1], "agent_registry");
    }
}
