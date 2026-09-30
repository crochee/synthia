//! Query parameter structs for v1 list endpoints.
//!
//! - [`PageQuery`]: generic cursor + limit + sort.
//! - [`SessionPageQuery`]: `PageQuery` flattened with `status` +
//!   `context_id` filters.
//!
//! `JobPageQuery` was removed in the 2026-08-15 optimization
//! pass: zero in-repo callers outside its own module (no
//! background-job endpoint was ever built — see `synthia-server`
//! routes for the actual `/api/v1/*` surface).
//! - [`DEFAULT_LIMIT`] = 20 — used when `limit` is `None`.
//! - [`MAX_LIMIT`] = 100 — `limit > MAX_LIMIT` is silently
//!   truncated (not an error). `limit == 0` IS an error
//!   (handled in [`super::cursor::resolve_page`]). The
//!   server aliases this to [`synthia::core::registry::MAX_LIMIT`]
//!   so the in-memory registry pagination cap and the
//!   wire-level cap share one source of truth — the
//!   previous "MUST stay in sync" warning is now a
//!   `const ... = synthia::core::registry::MAX_LIMIT;` line
//!   the compiler enforces.
//!
//! The alias uses an explicit `pub const X = synthia::core::Y`
//! rather than a `pub use` re-export so the symbol stays
//! at `synthia_server::api::v1::page_query::MAX_LIMIT` for
//! downstream callers (the server's public surface), and
//! the value tracks the core's value automatically.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Default page size when the client omits `limit`.
pub const DEFAULT_LIMIT: u64 = 20;

/// Maximum page size; larger values are silently truncated.
///
/// Sourced from [`synthia::core::registry::MAX_LIMIT`] so the
/// wire-level cap and the in-memory registry pagination cap
/// cannot drift. The two share a single value; a refactor
/// that changed the core's value would change the server's
/// value through the alias and fail the
/// `constants_match_spec` test below.
pub const MAX_LIMIT: u64 = synthia::core::registry::MAX_LIMIT;

/// Deserialize an optional `u64` that may arrive as a JSON number or as
/// a string.
///
/// A URL query string has no type information — `serde_urlencoded` hands
/// every value over as a string. A plain `Option<u64>` field works only
/// when serde can still coerce, which it cannot through
/// `#[serde(flatten)]`: flattening routes the input through serde's
/// buffering `Content` deserializer, where a `u64` demands a genuine
/// number and `?limit=20` fails with
/// `invalid type: string "20", expected u64` (HTTP 400). Accepting both
/// representations makes the same field work for query strings and for
/// JSON bodies (the OpenAPI/JSON callers the type is also documented
/// for).
mod de {
    use serde::{Deserialize, Deserializer};

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumberOrString {
        Number(u64),
        String(String),
    }

    pub(super) fn optional_u64<'de, D>(
        deserializer: D,
    ) -> Result<Option<u64>, D::Error>
    where
        D: Deserializer<'de>,
    {
        match Option::<NumberOrString>::deserialize(deserializer)? {
            None => Ok(None),
            Some(NumberOrString::Number(n)) => Ok(Some(n)),
            Some(NumberOrString::String(s)) => {
                s.trim().parse::<u64>().map(Some).map_err(|_| {
                    serde::de::Error::custom(format!(
                        "expected an integer, got `{s}`"
                    ))
                })
            }
        }
    }
}

/// Generic pagination query: cursor + limit + sort.
///
/// All fields are optional. `limit = 0` is rejected at the
/// handler layer (HTTP 400 `bad_request`). `limit > MAX_LIMIT`
/// is silently truncated to `MAX_LIMIT`. `sort` uses field name
/// with `-` prefix for descending order (e.g. `-created_at`).
#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    JsonSchema,
    validator::Validate,
)]
pub struct PageQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[validate(length(min = 1, message = "must not be empty"))]
    pub cursor: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de::optional_u64"
    )]
    pub limit: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
}

impl PageQuery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_cursor(mut self, cursor: impl Into<String>) -> Self {
        self.cursor = Some(cursor.into());
        self
    }

    pub fn with_limit(mut self, limit: impl Into<Option<u64>>) -> Self {
        self.limit = limit.into();
        self
    }

    pub fn with_sort(mut self, sort: impl Into<String>) -> Self {
        self.sort = Some(sort.into());
        self
    }
}

/// Session list query: [`PageQuery`] + `status` + `context_id`
/// filters.
///
/// `status` is a free-form string at this layer — the
/// `/api/v1/sessions` route validates it against the canonical
/// session-state set (`working` / `completed` / `failed` /
/// `canceled` / `input-required`) and returns HTTP 400
/// `bad_request` for unknown values.
#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    JsonSchema,
    validator::Validate,
)]
pub struct SessionPageQuery {
    #[serde(flatten)]
    pub page: PageQuery,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
}

impl SessionPageQuery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_page(mut self, page: PageQuery) -> Self {
        self.page = page;
        self
    }

    pub fn with_status(mut self, status: impl Into<String>) -> Self {
        self.status = Some(status.into());
        self
    }

    pub fn with_context_id(mut self, context_id: impl Into<String>) -> Self {
        self.context_id = Some(context_id.into());
        self
    }
}

impl From<PageQuery> for SessionPageQuery {
    fn from(page: PageQuery) -> Self {
        Self {
            page,
            status: None,
            context_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- PageQuery ---

    #[test]
    fn page_query_default_all_none() {
        let q = PageQuery::default();
        assert!(q.cursor.is_none());
        assert!(q.limit.is_none());
        assert!(q.sort.is_none());
    }

    #[test]
    fn page_query_builders_set_fields() {
        let q = PageQuery::new()
            .with_cursor("abc")
            .with_limit(50u64)
            .with_sort("-created_at");
        assert_eq!(q.cursor.as_deref(), Some("abc"));
        assert_eq!(q.limit, Some(50));
        assert_eq!(q.sort.as_deref(), Some("-created_at"));
    }

    #[test]
    fn page_query_serializes_omitting_null_fields() {
        let q = PageQuery::default();
        let json = serde_json::to_string(&q).unwrap();
        assert_eq!(json, "{}");
    }

    #[test]
    fn page_query_serializes_present_fields() {
        let q = PageQuery::new().with_cursor("c").with_limit(10u64);
        let json = serde_json::to_string(&q).unwrap();
        assert_eq!(json, r#"{"cursor":"c","limit":10}"#);
    }

    #[test]
    fn page_query_deserializes_partial() {
        let q: PageQuery = serde_json::from_str(r#"{"limit":5}"#).unwrap();
        assert_eq!(q.limit, Some(5));
        assert!(q.cursor.is_none());
        assert!(q.sort.is_none());
    }

    #[test]
    fn constants_match_spec() {
        assert_eq!(DEFAULT_LIMIT, 20);
        assert_eq!(MAX_LIMIT, 100);
        // The server's MAX_LIMIT is sourced from
        // synthia::core::registry::MAX_LIMIT; pin the equality
        // so a refactor that broke the alias would fail here
        // (the core's `max_limit_matches_spec` pins the value
        // itself; this test pins the alias).
        assert_eq!(MAX_LIMIT, synthia::core::registry::MAX_LIMIT);
    }

    // --- SessionPageQuery ---

    #[test]
    fn session_page_query_default_all_none() {
        let q = SessionPageQuery::default();
        assert!(q.page.cursor.is_none());
        assert!(q.status.is_none());
        assert!(q.context_id.is_none());
    }

    #[test]
    fn session_page_query_builders_set_fields() {
        let q = SessionPageQuery::new()
            .with_page(PageQuery::new().with_limit(5u64))
            .with_status("working")
            .with_context_id("ctx_1");
        assert_eq!(q.page.limit, Some(5));
        assert_eq!(q.status.as_deref(), Some("working"));
        assert_eq!(q.context_id.as_deref(), Some("ctx_1"));
    }

    #[test]
    fn session_page_query_flattens_page_on_serialize() {
        let q = SessionPageQuery::new()
            .with_page(PageQuery::new().with_cursor("c").with_limit(5u64))
            .with_status("done");
        let json = serde_json::to_string(&q).unwrap();
        // Flattened: cursor/limit at top level, status alongside.
        assert!(json.contains("\"cursor\":\"c\""));
        assert!(json.contains("\"limit\":5"));
        assert!(json.contains("\"status\":\"done\""));
        assert!(!json.contains("\"page\""));
    }

    #[test]
    fn session_page_query_deserializes_with_flattened_fields() {
        let json =
            r#"{"cursor":"c","limit":5,"status":"working","context_id":"x"}"#;
        let q: SessionPageQuery = serde_json::from_str(json).unwrap();
        assert_eq!(q.page.cursor.as_deref(), Some("c"));
        assert_eq!(q.page.limit, Some(5));
        assert_eq!(q.status.as_deref(), Some("working"));
        assert_eq!(q.context_id.as_deref(), Some("x"));
    }

    /// A URL query string has no type information: every value arrives
    /// as a string, and `#[serde(flatten)]` buffers through serde's
    /// `Content` deserializer where a bare `u64` field would refuse
    /// `limit=20` with `invalid type: string "20", expected u64`
    /// (HTTP 400). Deserialization goes through `axum::extract::Query`
    /// here because that is the exact seam the routes use.
    #[test]
    fn session_page_query_accepts_a_urlencoded_string_limit() {
        let q = axum::extract::Query::<SessionPageQuery>::try_from_uri(
            &"/api/v1/sessions?cursor=c&limit=20&status=working"
                .parse()
                .unwrap(),
        )
        .unwrap()
        .0;
        assert_eq!(q.page.cursor.as_deref(), Some("c"));
        assert_eq!(q.page.limit, Some(20));
        assert_eq!(q.status.as_deref(), Some("working"));
    }

    /// The same holds for the un-flattened cursor + limit + sort query
    /// the other list endpoints use.
    #[test]
    fn page_query_accepts_a_urlencoded_string_limit() {
        let q = axum::extract::Query::<PageQuery>::try_from_uri(
            &"/api/v1/tools?limit=50&sort=-created_at".parse().unwrap(),
        )
        .unwrap()
        .0;
        assert_eq!(q.limit, Some(50));
        assert_eq!(q.sort.as_deref(), Some("-created_at"));

        // A missing limit stays `None` rather than becoming an error.
        let q = axum::extract::Query::<PageQuery>::try_from_uri(
            &"/api/v1/tools?sort=name".parse().unwrap(),
        )
        .unwrap()
        .0;
        assert_eq!(q.limit, None);
    }

    /// A non-numeric `limit` is still rejected — as a 400 the error
    /// adapter can report, not a silently ignored parameter.
    #[test]
    fn page_query_rejects_a_non_numeric_limit() {
        let err = axum::extract::Query::<PageQuery>::try_from_uri(
            &"/api/v1/tools?limit=abc".parse().unwrap(),
        )
        .unwrap_err();
        assert!(
            format!("{err:?}").contains("expected an integer"),
            "unhelpful rejection: {err:?}"
        );
    }

    #[test]
    fn session_page_query_from_page_query_preserves_page() {
        let page = PageQuery::new().with_limit(15u64);
        let q: SessionPageQuery = page.into();
        assert_eq!(q.page.limit, Some(15));
        assert!(q.status.is_none());
        assert!(q.context_id.is_none());
    }

    // --- JobPageQuery tests removed along with the struct in
    //     the 2026-08-15 optimization pass (zero in-repo callers).
}
