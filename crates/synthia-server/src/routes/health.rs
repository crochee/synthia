use std::sync::Arc;

use axum::{
    Json,
    body::{Body, Bytes},
    extract::State,
    http::{
        HeaderMap,
        HeaderValue,
        StatusCode,
        header::{CACHE_CONTROL, CONTENT_TYPE, ETAG, IF_NONE_MATCH},
    },
    response::{IntoResponse, Response},
};
use parking_lot::RwLock;
use serde::Serialize;
use synthia::telemetry::TEXT_EXPOSITION_CONTENT_TYPE;

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

/// `GET /version` response body.
///
/// Captures the five values that `synthia_server::build_info` bakes
/// into the binary at compile time. The route is mounted on the
/// **public** router next to `/livez` and `/readyz` because version
/// checks are part of the orchestrator/operator surface, not the
/// chat API: a Kubernetes rollout needs `kubectl exec pod -- curl
/// http://localhost/version` to confirm the new build landed.
#[derive(Serialize)]
pub struct VersionResponse {
    /// Cargo package version (e.g. `0.1.0`).
    pub version: &'static str,
    /// Full git commit SHA the binary was built from, or `"unknown"`
    /// when `SYNTHIA_GIT_SHA` was unset at compile time.
    pub git_sha: &'static str,
    /// First 7 hex chars of `git_sha`, for display.
    pub short_sha: &'static str,
    /// ISO-8601 wall-clock of the build moment, or `"unknown"`.
    pub build_time: &'static str,
    /// Target triple the binary was compiled for.
    pub target: &'static str,
    /// Compile profile (`release` for CI builds, `debug` for `cargo run`).
    pub profile: &'static str,
    /// One-line summary in `git describe --long --dirty` style.
    pub long_version: String,
}

/// `GET /version` — return the baked build identity.
///
/// Companion to `/livez` / `/readyz`: it answers "which exact
/// binary is this process?" The fields are `&'static str` because
/// they are baked at compile time via `option_env!`, so the route
/// touches no shared state and never blocks.
///
/// `Cache-Control: no-store` is intentional: a `kubectl rollout`
/// poll loop that gets a cached `200` would silently never see a
/// new build.
pub async fn version() -> Response {
    (
        StatusCode::OK,
        [(CACHE_CONTROL, HeaderValue::from_static("no-store"))],
        Json(VersionResponse {
            version: crate::build_info::VERSION,
            git_sha: crate::build_info::GIT_SHA,
            short_sha: crate::build_info::short_sha(),
            build_time: crate::build_info::BUILD_TIME,
            target: crate::build_info::TARGET,
            profile: crate::build_info::PROFILE,
            long_version: crate::build_info::long_version(),
        }),
    )
        .into_response()
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

/// FNV-1a (64-bit) offset basis and prime.
///
/// The tag needs a content fingerprint, not a cryptographic
/// digest, and the tree carries no hashing crate for either: this
/// is twenty lines, allocates nothing, and a collision only ever
/// costs one client one stale model list — there is no adversary
/// picking workspace configs.
const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// [`std::io::Write`] sink folding every byte into an FNV-1a hash.
///
/// [`serde_json::to_writer`] hands `write` the exact bytes a `200`
/// would send, so the ETag is derived from the serialized body
/// without materializing a scratch copy of it — only a cache miss
/// needs the bytes themselves.
struct Fnv1aWriter(u64);

impl Fnv1aWriter {
    fn new() -> Self {
        Self(FNV_OFFSET_BASIS)
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

impl std::io::Write for Fnv1aWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0 = buf.iter().fold(self.0, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(FNV_PRIME)
        });
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// ETag for a `/api/models` body.
///
/// The tag is a content hash of the *serialized* response, so it
/// covers every field a client receives — each provider entry as
/// well as `default_provider` and `default_model`. Deriving it from
/// the default model name alone (the previous shape) left a
/// provider-set change invisible: a revalidating client got a `304`
/// and stayed pinned to a model list the server no longer served.
///
/// `CARGO_PKG_VERSION` stays in the tag as a deploy-busting
/// component, so a rebuild with byte-identical config still
/// invalidates clients.
///
/// `W/` marks the validator weak (RFC 9110 §8.8.3): the tag
/// asserts that two representations are semantically equivalent
/// (equal config ⇒ equal model list), not that they are
/// byte-identical entities.
fn models_etag(body: &ModelsResponse) -> String {
    let mut hash = Fnv1aWriter::new();
    // `ModelsResponse` is `String` / `usize` / `bool` throughout,
    // so serialization into the infallible sink cannot fail: no
    // float, no map key, no I/O. A tag that silently skipped the
    // body it describes could never invalidate a client, so a
    // failure here is a programming error worth surfacing.
    serde_json::to_writer(&mut hash, body)
        .expect("ModelsResponse is JSON-serializable");
    format!(
        "W/\"models-{}-{:016x}\"",
        env!("CARGO_PKG_VERSION"),
        hash.finish(),
    )
}

/// The `W/` weak-validator prefix (RFC 9110 §8.8.3).
const WEAK_PREFIX: &str = "W/";

/// An entity-tag with surrounding whitespace and any `W/` prefix
/// removed.
///
/// `If-None-Match` is evaluated with the *weak* comparison function
/// (RFC 9110 §13.1.2), which ignores the prefix on both sides: a
/// weak tag asserts semantic equivalence, so `W/"x"` and `"x"` must
/// select the same representation.
fn normalize_etag(tag: &str) -> &str {
    let tag = tag.trim();
    tag.strip_prefix(WEAK_PREFIX).unwrap_or(tag)
}

/// Does the request's `If-None-Match` select `etag`?
///
/// The header is a comma-separated list of entity-tags, each of
/// which may carry a `W/` prefix; a bare `*` matches any current
/// representation. A missing, unparsable, or non-matching header
/// leaves the precondition unmet and the caller serves the body:
/// answering `304` to *any* `If-None-Match` — the shape this
/// replaces — pinned every client that had cached one model list to
/// it, `providers` changes and redeploys included.
fn if_none_match_selects(req: &HeaderMap, etag: &str) -> bool {
    let Some(header) = req.get(IF_NONE_MATCH) else {
        return false;
    };
    let Ok(header) = header.to_str() else {
        return false;
    };
    let current = normalize_etag(etag);
    header.split(',').any(|candidate| {
        candidate.trim() == "*" || normalize_etag(candidate) == current
    })
}

/// Cached `(etag, body)` pair for `/api/models`.
///
/// The `ModelsResponse` body is fully determined at startup
/// (`workspace_config.providers` + `default_model` +
/// `default_provider`) and never mutates over the binary's
/// lifetime — a hot-reload would replace the entire `AppState`
/// `Arc`. Without this cache every `/api/models` request rebuilt
/// the full `Vec<ModelEntry>` (one String per field × per
/// provider) and re-materialized the JSON body.
static MODELS_CACHE: RwLock<Option<Arc<CachedModels>>> = RwLock::new(None);

/// One cached `/api/models` representation, keyed by its own ETag.
///
/// The key is a content hash of the body, not the default model
/// name it used to be: two `AppState`s in one process (a hot
/// reload, or two states in one test binary) sharing a
/// `default_model` but not their `providers` collided on the old
/// key and were served the first state's body.
struct CachedModels {
    etag: String,
    body: Bytes,
}

/// Resolve the cached representation of `body`, building it only on
/// a miss.
///
/// Warm path: hash the candidate body and reuse the stored bytes,
/// skipping JSON materialization entirely. Cold path: serialize the
/// bytes the `200` sends — the very bytes the ETag describes — and
/// store them, re-checking the key inside the write guard so a
/// request that raced this one, or one holding a different
/// `AppState`, can never be served a body built from another
/// state's config.
fn cached_models(body: &ModelsResponse) -> Arc<CachedModels> {
    let etag = models_etag(body);
    {
        let guard = MODELS_CACHE.read();
        let hit = guard.as_ref().filter(|c| c.etag == etag).cloned();
        if let Some(hit) = hit {
            return hit;
        }
    }
    let bytes =
        serde_json::to_vec(body).expect("ModelsResponse is JSON-serializable");
    let mut guard = MODELS_CACHE.write();
    let hit = guard.as_ref().filter(|c| c.etag == etag).cloned();
    if let Some(hit) = hit {
        return hit;
    }
    let fresh = Arc::new(CachedModels {
        etag,
        body: Bytes::from(bytes),
    });
    *guard = Some(fresh.clone());
    fresh
}

/// GET /metrics - Prometheus text exposition endpoint.
///
/// Available only behind the `metrics` cargo feature. Returns the
/// aggregated `prometheus::gather()` payload in the standard text
/// exposition format (version 0.0.4), suitable for scraping by a
/// Prometheus server.
///
/// Like `/livez` and `/readyz`, this endpoint is intentionally
/// mounted OUTSIDE the auth / trace-context / RED metrics layers:
/// - Not tracked itself by the metrics middleware (a scrape that
///   counts itself is noise).
/// - Not auth-protected (Prometheus servers scrape anonymously).
/// - No `traceparent` minted (orchestrator probes flood the trace
///   pipeline).
pub async fn metrics() -> Response {
    let body = synthia::telemetry::gather_text();
    (
        StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            HeaderValue::from_static(TEXT_EXPOSITION_CONTENT_TYPE),
        )],
        body,
    )
        .into_response()
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
///
/// `no-cache` *forces* revalidation, so the conditional path must
/// compare this request's `If-None-Match` against the current tag:
/// answering `304` to any `If-None-Match` at all would pin a client
/// that had cached one body to it for as long as the directive
/// stands, provider redeploys included.
pub async fn list_models(
    State(state): State<Arc<AppState>>,
    req: axum::http::HeaderMap,
) -> Response {
    let config = &state.workspace_config;

    // Build-once cache: both the ETag and the serialized body are
    // fully determined by `workspace_config`, which is loaded once
    // at startup and never mutated. Reusing the cached
    // `(etag, body)` entry across requests skips the per-request
    // `Vec<ModelEntry>` rebuild (N providers × 4 strings each).
    let cached = cached_models(&ModelsResponse {
        models: config
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
                supports_streaming: entry.supports_streaming.unwrap_or(true),
            })
            .collect(),
        default_provider: config.default_provider.clone(),
        default_model: config.default_model.clone(),
    });

    let etag_value = HeaderValue::from_str(&cached.etag)
        .unwrap_or_else(|_| HeaderValue::from_static("W/\"models-invalid\""));

    let mut headers = HeaderMap::new();
    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static("no-cache, max-age=60"),
    );
    headers.insert(ETAG, etag_value);

    // Conditional GET: a request carrying the current tag — or `*`
    // — reuses its cached copy, so the body bytes never reach the
    // response. Any other tag falls through to the full `200`: the
    // tag is a hash of the body, so a mismatch means the client's
    // copy really is stale.
    if if_none_match_selects(&req, &cached.etag) {
        return (StatusCode::NOT_MODIFIED, headers).into_response();
    }

    // These bytes *are* the serialization `Json` would produce (the
    // ETag is their hash), so they ship as-is instead of being
    // rebuilt from the struct on every request.
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    let body = Body::from(cached.body.clone());
    (StatusCode::OK, headers, body).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hermetic `AppState` for route tests.
    ///
    /// `AppState::new` loads the workspace configuration and builds a
    /// real provider, so it fails without ambient provider
    /// credentials by design (`WorkspaceConfig::load_from_dir` refuses
    /// to start a server with no provider). Route behaviour has
    /// nothing to do with which provider is configured, so these tests
    /// build the state over in-memory components and a `FakeProvider`
    /// instead — with a temp workspace whose lifetime the caller
    /// keeps.
    async fn test_state()
    -> (std::sync::Arc<crate::state::AppState>, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("temp workspace");
        let sessions = synthia::session::manager::SessionRegistry::new(
            dir.path().join("sessions"),
        );
        let state = crate::state::AppState::for_test(
            sessions,
            dir.path().to_path_buf(),
        )
        .await;
        (std::sync::Arc::new(state), dir)
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

    /// A minimal `/api/models` body for ETag tests.
    fn body_for(default_model: &str, provider_model: &str) -> ModelsResponse {
        ModelsResponse {
            models: vec![ModelEntry {
                provider: "openai".to_string(),
                model: provider_model.to_string(),
                context_window: 128_000,
                supports_tools: true,
                supports_streaming: true,
            }],
            default_provider: "openai".to_string(),
            default_model: default_model.to_string(),
        }
    }

    #[test]
    fn models_etag_is_stable_per_default_model() {
        // Same body → same output, byte-for-byte.
        assert_eq!(
            models_etag(&body_for("gpt-4", "gpt-4")),
            models_etag(&body_for("gpt-4", "gpt-4"))
        );
    }

    #[test]
    fn models_etag_differs_when_default_model_changes() {
        assert_ne!(
            models_etag(&body_for("gpt-4", "gpt-4")),
            models_etag(&body_for("claude-opus-4", "gpt-4")),
            "changing the default model must invalidate the ETag"
        );
    }

    /// The tag hashes the whole body, so a provider entry that
    /// moves while `default_model` stays put still invalidates the
    /// client. Folding `default_model` alone (the old shape) let a
    /// redeploy onto a different provider set keep answering `304`.
    #[test]
    fn models_etag_covers_providers_and_default_provider() {
        let base = body_for("gpt-4", "gpt-4");

        let mut other_model = base.clone();
        other_model.models[0].model = "gpt-4o".to_string();
        assert_ne!(
            models_etag(&base),
            models_etag(&other_model),
            "changing a provider entry must invalidate the ETag"
        );

        let mut other_provider = base.clone();
        other_provider.default_provider = "anthropic".to_string();
        assert_ne!(
            models_etag(&base),
            models_etag(&other_provider),
            "changing the default provider must invalidate the ETag"
        );
    }

    #[test]
    fn models_etag_embeds_package_version() {
        // The binary version is a baked-in constant, so the tag
        // must include it as a deploy-busting component.
        let tag = models_etag(&body_for("gpt-4", "gpt-4"));
        assert!(
            tag.contains(env!("CARGO_PKG_VERSION")),
            "ETag must include CARGO_PKG_VERSION, got: {tag}"
        );
    }

    /// `If-None-Match` is a comma-separated list of entity-tags,
    /// each of which may carry the `W/` prefix, and `*` selects any
    /// current representation. Only such a selection may answer
    /// `304`; anything else leaves the precondition unmet.
    #[test]
    fn if_none_match_selects_only_on_matching_tag() {
        let etag = models_etag(&body_for("gpt-4", "gpt-4"));
        let strong = etag.strip_prefix("W/").expect("weak prefix");

        for (value, expected) in [
            (etag.as_str(), true),
            (strong, true),
            ("*", true),
            ("* , W/\"other\"", true),
            ("W/\"other\", W/\"other-2\"", false),
            ("W/\"other\"", false),
            ("", false),
        ] {
            let mut req = HeaderMap::new();
            req.insert(IF_NONE_MATCH, HeaderValue::from_str(value).unwrap());
            assert_eq!(
                if_none_match_selects(&req, &etag),
                expected,
                "If-None-Match: {value:?}"
            );
        }

        // No header at all is not a selection either.
        assert!(!if_none_match_selects(&HeaderMap::new(), &etag));
    }

    /// The cache key is the body's ETag, not the default model
    /// name: a body whose provider set changed while
    /// `default_model` did not must never be answered from the
    /// previous entry — that collision is what served a second
    /// `AppState` the first state's models.
    #[test]
    fn models_cache_keys_on_body_content() {
        let base = body_for("gpt-4", "gpt-4");
        let mut swapped = base.clone();
        swapped.models[0].model = "gpt-4o".to_string();

        let first = cached_models(&base);
        let second = cached_models(&swapped);
        assert_eq!(first.etag, models_etag(&base));
        assert_eq!(second.etag, models_etag(&swapped));
        assert_ne!(first.body, second.body);

        // Equal bodies keep hitting the same key, whatever a
        // parallel test did to the single-entry cache meanwhile.
        assert_eq!(cached_models(&base).body, first.body);
    }

    /// `list_models` with no `If-None-Match` MUST return 200 with
    /// the full JSON body and the new ETag + Cache-Control
    /// headers. Pinning the wire shape here guards against a
    /// missing-header-propagation regression.
    #[tokio::test]
    async fn list_models_emits_etag_and_cache_control() {
        let (state, _dir) = test_state().await;
        let expected_default = state.workspace_config.default_model.clone();
        let req = axum::http::HeaderMap::new();
        let resp = list_models(State(state), req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get(CACHE_CONTROL).unwrap(),
            "no-cache, max-age=60"
        );
        assert_eq!(
            resp.headers().get(CONTENT_TYPE).unwrap(),
            "application/json"
        );
        let etag = resp.headers().get(ETAG).unwrap().to_str().unwrap();
        assert!(etag.starts_with("W/\"models-"), "ETag shape: {etag}");
        assert!(
            etag.contains(env!("CARGO_PKG_VERSION")),
            "ETag must embed CARGO_PKG_VERSION: {etag}"
        );

        use http_body_util::BodyExt;
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(body["models"].is_array(), "got: {body}");
        assert_eq!(body["default_model"], expected_default.as_str());
    }

    /// Conditional GET MUST compare rather than short-circuit: the
    /// tag the handler itself just emitted revalidates to an empty
    /// `304`, while any other tag is served the full `200`. The
    /// handler used to answer `304` to *any* `If-None-Match`, which
    /// — on top of `Cache-Control: no-cache` — pinned a browser
    /// that had cached one model list to it forever.
    #[tokio::test]
    async fn list_models_returns_304_only_for_the_current_etag() {
        let (state, _dir) = test_state().await;

        let first = list_models(State(state.clone()), HeaderMap::new()).await;
        assert_eq!(first.status(), StatusCode::OK);
        let etag = first.headers().get(ETAG).unwrap().clone();
        let strong = {
            let tag = etag.to_str().unwrap();
            let tag = tag.strip_prefix("W/").unwrap_or(tag);
            HeaderValue::from_str(tag).unwrap()
        };

        for value in [etag, strong, HeaderValue::from_static("*")] {
            let mut req = HeaderMap::new();
            req.insert(IF_NONE_MATCH, value.clone());
            let resp = list_models(State(state.clone()), req).await;
            assert_eq!(
                resp.status(),
                StatusCode::NOT_MODIFIED,
                "If-None-Match: {value:?} must revalidate"
            );
            // Body MUST be empty on a 304 (RFC 9110 §15.4.5).
            // `Body::size_hint` returns an upper bound; an empty
            // body reports exact size 0 via the `SizeHint` struct.
            use axum::body::HttpBody;
            let size = resp.into_body().size_hint().exact();
            assert_eq!(size, Some(0), "304 response body must be empty");
        }

        // A tag for a different representation is not a match: the
        // client gets the body, not a stale 304.
        let mut req = HeaderMap::new();
        req.insert(
            IF_NONE_MATCH,
            HeaderValue::from_static("W/\"models-test-irrelevant\""),
        );
        let resp = list_models(State(state), req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        use http_body_util::BodyExt;
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        assert!(
            serde_json::from_slice::<serde_json::Value>(&bytes).is_ok(),
            "a mismatched If-None-Match must be served the body"
        );
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

    /// `readyz` MUST return 200 once every readiness check
    /// passes. The agent-registry check passes because
    /// `AppState::new` registers the canonical ReAct agent.
    /// Reserved for the future; the chat surface is the only
    /// check; it was retired (kept for the future).
    #[tokio::test]
    async fn readyz_is_200_after_state_initialized() {
        let (state, _dir) = test_state().await;
        let resp = readyz(State(state)).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.headers().get(CACHE_CONTROL).unwrap(), "no-store");
        use http_body_util::BodyExt;
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["status"], "ok");
        assert!(
            body.get("failed").is_none(),
            "readyz must report no failed checks once AppState is initialised, got: {body}"
        );
    }

    /// `ProbeResponse` with failed checks MUST serialize the
    /// names so operators can see *why* the probe fails from the
    /// response body alone.
    #[test]
    fn probe_response_serializes_failed_check_names() {
        let resp = ProbeResponse {
            status: "unavailable",
            failed: vec!["agent_registry"],
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["status"], "unavailable");
        assert_eq!(json["failed"][0], "agent_registry");
    }

    /// `/metrics` MUST return 200 with the Prometheus text
    /// exposition MIME and a body containing the registered RED
    /// metric families (after a labeled child has been observed).
    /// Available only behind the `metrics` cargo feature. Uses
    /// the global prometheus registry's lazy initialization by
    /// observing a sample first — without a child,
    /// `prometheus::gather()` drops childless families by design.
    #[tokio::test]
    async fn metrics_endpoint_serves_prometheus_text() {
        // Seed at least one labeled sample so the family has a
        // child — `prometheus::gather()` drops childless families
        // by design, and the test must observe a populated body.
        // Use a unique label so the test is order-independent
        // when the global registry is shared with other tests.
        let label_path = "/__metrics_endpoint_probe__";
        synthia::telemetry::HTTP_REQUESTS_TOTAL
            .with_label_values(&["GET", label_path])
            .inc();

        let response = metrics().await;
        assert_eq!(response.status(), StatusCode::OK);
        let mime = response
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .expect("content-type");
        assert_eq!(mime, TEXT_EXPOSITION_CONTENT_TYPE);

        use http_body_util::BodyExt;
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let text = std::str::from_utf8(&bytes).expect("utf-8");
        assert!(
            text.contains("http_requests_total"),
            "expected http_requests_total family in scrape, got: {text}"
        );
        assert!(
            text.contains(label_path),
            "expected labeled child `{label_path}` in scrape, got: {text}"
        );
    }
}
