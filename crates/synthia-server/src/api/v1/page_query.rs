//! Query parameter structs for v1 list endpoints.
//!
//! - [`PageQuery`]: generic cursor + limit + sort.
//! - [`TaskPageQuery`]: `PageQuery` flattened with `status` +
//!   `context_id` filters.
//!
//! `JobPageQuery` was removed in the 2026-08-15 optimization
//! pass: zero in-repo callers outside its own module (no
//! background-job endpoint was ever built — see `synthia-server`
//! routes for the actual `/api/v1/*` surface).
//!
//! # Constants
//!
//! - [`DEFAULT_LIMIT`] = 20 — used when `limit` is `None`.
//! - [`MAX_LIMIT`] = 100 — `limit > MAX_LIMIT` is silently
//!   truncated (not an error). `limit == 0` IS an error
//!   (handled in [`super::cursor::resolve_page`]).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Default page size when the client omits `limit`.
pub const DEFAULT_LIMIT: u64 = 20;

/// Maximum page size; larger values are silently truncated.
pub const MAX_LIMIT: u64 = 100;

/// Generic pagination query: cursor + limit + sort.
///
/// All fields are optional. `limit = 0` is rejected at the
/// handler layer (HTTP 400 `bad_request`). `limit > MAX_LIMIT`
/// is silently truncated to `MAX_LIMIT`. `sort` uses field name
/// with `-` prefix for descending order (e.g. `-created_at`).
#[derive(
    Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema,
)]
pub struct PageQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
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

/// Task list query: [`PageQuery`] + `status` + `context_id`
/// filters.
///
/// `status` is a free-form string at this layer — handlers
/// validate it against the A2A `TaskState` enum and return
/// HTTP 400 `bad_request` for unknown values.
#[derive(
    Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema,
)]
pub struct TaskPageQuery {
    #[serde(flatten)]
    pub page: PageQuery,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
}

impl TaskPageQuery {
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

impl From<PageQuery> for TaskPageQuery {
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
    }

    // --- TaskPageQuery ---

    #[test]
    fn task_page_query_default_all_none() {
        let q = TaskPageQuery::default();
        assert!(q.page.cursor.is_none());
        assert!(q.status.is_none());
        assert!(q.context_id.is_none());
    }

    #[test]
    fn task_page_query_builders_set_fields() {
        let q = TaskPageQuery::new()
            .with_page(PageQuery::new().with_limit(5u64))
            .with_status("working")
            .with_context_id("ctx_1");
        assert_eq!(q.page.limit, Some(5));
        assert_eq!(q.status.as_deref(), Some("working"));
        assert_eq!(q.context_id.as_deref(), Some("ctx_1"));
    }

    #[test]
    fn task_page_query_flattens_page_on_serialize() {
        let q = TaskPageQuery::new()
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
    fn task_page_query_deserializes_with_flattened_fields() {
        let json =
            r#"{"cursor":"c","limit":5,"status":"working","context_id":"x"}"#;
        let q: TaskPageQuery = serde_json::from_str(json).unwrap();
        assert_eq!(q.page.cursor.as_deref(), Some("c"));
        assert_eq!(q.page.limit, Some(5));
        assert_eq!(q.status.as_deref(), Some("working"));
        assert_eq!(q.context_id.as_deref(), Some("x"));
    }

    #[test]
    fn task_page_query_from_page_query_preserves_page() {
        let page = PageQuery::new().with_limit(15u64);
        let q: TaskPageQuery = page.into();
        assert_eq!(q.page.limit, Some(15));
        assert!(q.status.is_none());
        assert!(q.context_id.is_none());
    }

    // --- JobPageQuery tests removed along with the struct in
    //     the 2026-08-15 optimization pass (zero in-repo callers).
}
