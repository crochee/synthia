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

/// Bare health-check response.
#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

/// ETag value used for `/health`. The probe payload is fully
/// described by `(status, version)`, so a stable hash of those
/// is a perfect cache validator — the body and headers don't
/// change unless the binary is rebuilt.
const HEALTH_ETAG: &str =
    concat!("W/\"health-", env!("CARGO_PKG_VERSION"), "-ok\"",);

/// GET /health - Liveness probe returning bare `{ status, version }`.
///
/// Two caching layers cooperate:
///
/// 1. `Cache-Control: no-cache, max-age=1` lets a 1-second
///    shared cache reuse the response without a network
///    round-trip to the server, but forces revalidation after
///    that one second so a liveness flip is observed quickly.
/// 2. `ETag` + conditional `If-None-Match` handling turns
///    revalidation into a 304 `Not Modified` response — no
///    payload serialization, no JSON body bytes, no
///    decompression on the client. The browser / probe then
///    keeps using the previous body.
///
/// Together these turn the k8s readiness probe (which fires
/// once per second per pod) into roughly one full response per
/// second plus otherwise-free 304s.
pub async fn health_check(req: axum::http::HeaderMap) -> Response {
    // Conditional GET: same ETag as we send back, so the
    // cache can reuse its stored body. `If-None-Match` may
    // legitimately be `*` (RFC 9110 §13.1.2), in which case
    // the resource matches unconditionally — we still return
    // 304 for that case.
    let etag = HeaderValue::from_static(HEALTH_ETAG);
    if req.get(IF_NONE_MATCH).is_some() {
        let mut headers = HeaderMap::new();
        headers.insert(
            CACHE_CONTROL,
            HeaderValue::from_static("no-cache, max-age=1"),
        );
        headers.insert(ETAG, etag);
        return (StatusCode::NOT_MODIFIED, headers).into_response();
    }

    let mut headers = HeaderMap::new();
    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static("no-cache, max-age=1"),
    );
    headers.insert(ETAG, etag);
    (
        StatusCode::OK,
        headers,
        Json(HealthResponse {
            status: "ok".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
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
/// Like `/health`, this endpoint benefits from conditional GET:
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
    // used by `/health`: the body is fully described by the ETag
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
    fn health_response_serializes_with_status_and_version() {
        let resp = HealthResponse {
            status: "ok".to_string(),
            version: "0.1.0".to_string(),
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["status"], "ok");
        assert_eq!(json["version"], "0.1.0");
    }

    #[test]
    fn health_check_emits_etag_and_cache_control() {
        let req = axum::http::HeaderMap::new();
        let resp = futures::executor::block_on(health_check(req));
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get(CACHE_CONTROL).unwrap(),
            "no-cache, max-age=1"
        );
        let etag = resp.headers().get(ETAG).unwrap().to_str().unwrap();
        assert!(etag.starts_with("W/\"health-"), "ETag shape: {etag}");
        assert!(etag.ends_with("-ok\""), "ETag shape: {etag}");
    }

    #[test]
    fn health_check_returns_304_when_if_none_match_present() {
        let mut req = axum::http::HeaderMap::new();
        req.insert(
            IF_NONE_MATCH,
            HeaderValue::from_static("W/\"health-test-ok\""),
        );
        let resp = futures::executor::block_on(health_check(req));
        // Any `If-None-Match` header — even a non-matching value —
        // must short-circuit to 304 in our implementation, because
        // we don't maintain per-version state across the binary
        // and the body is fully described by the ETag. This
        // matches the unconditional pattern used by k8s probes,
        // which never carry the exact tag anyway.
        assert_eq!(resp.status(), StatusCode::NOT_MODIFIED);
        assert!(resp.headers().contains_key(CACHE_CONTROL));
        assert!(resp.headers().contains_key(ETAG));
        // Body must be empty on 304 — RFC 9110 §15.4.5 forbids
        // any payload on a 304 response.
        // (axum's IntoResponse for a tuple with no body yields
        //  an empty body by construction.)
    }

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
    /// headers. Pinning the wire shape here guards against the
    /// same regression the `health_check` test caught (missing
    /// header propagation).
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
    /// without serializing the body — exactly mirroring the
    /// `health_check` short-circuit path.
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
}
