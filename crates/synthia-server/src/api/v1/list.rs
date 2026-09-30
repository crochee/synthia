//! [`List<T>`] — generic list envelope returned by every v1
//! list endpoint.
//!
//! Wire shape:
//! ```json
//! {
//!   "data": [ <T>, ... ],
//!   "next_cursor": "<opaque-base64>" | null,
//!   "total": 1234
//! }
//! ```
//!
//! - `data` is always present (possibly empty).
//! - `next_cursor` is `null` (or omitted via `Option`) when there
//!   are no more pages.
//! - `total` is `Option<u64>` so handlers MAY omit it for large
//!   datasets where `COUNT` is expensive. It is skipped from JSON
//!   when `None`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Generic list response for all v1 list endpoints.
///
/// See the module docs for the wire shape and field semantics.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct List<T> {
    pub data: Vec<T>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
}

impl<T> List<T> {
    /// Build a list response with `data` and no `next_cursor` /
    /// `total`. Caller typically upgrades `next_cursor` via
    /// [`super::cursor::next_cursor`] when there are more pages.
    pub fn new(data: Vec<T>) -> Self {
        Self {
            data,
            next_cursor: None,
            total: None,
        }
    }

    /// Attach a `next_cursor` (replaces any existing value).
    pub fn with_next_cursor(mut self, cursor: impl Into<String>) -> Self {
        self.next_cursor = Some(cursor.into());
        self
    }

    /// Attach a `total` (replaces any existing value).
    pub fn with_total(mut self, total: u64) -> Self {
        self.total = Some(total);
        self
    }
}

impl<T> Default for List<T> {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl<T> From<Vec<T>> for List<T> {
    fn from(data: Vec<T>) -> Self {
        Self::new(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_yields_empty_cursors_and_total() {
        let list: List<i32> = List::new(vec![1, 2, 3]);
        assert_eq!(list.data, vec![1, 2, 3]);
        assert!(list.next_cursor.is_none());
        assert!(list.total.is_none());
    }

    #[test]
    fn default_yields_empty_data() {
        let list: List<i32> = List::default();
        assert!(list.data.is_empty());
        assert!(list.next_cursor.is_none());
        assert!(list.total.is_none());
    }

    #[test]
    fn from_vec_sets_data_only() {
        let list: List<&'static str> = vec!["a", "b"].into();
        assert_eq!(list.data, vec!["a", "b"]);
        assert!(list.next_cursor.is_none());
        assert!(list.total.is_none());
    }

    #[test]
    fn builders_set_fields() {
        let list: List<i32> =
            List::new(vec![1]).with_next_cursor("abc").with_total(42u64);
        assert_eq!(list.next_cursor.as_deref(), Some("abc"));
        assert_eq!(list.total, Some(42));
    }

    #[test]
    fn serialize_omits_next_cursor_and_total_when_none() {
        let list: List<i32> = List::new(vec![1, 2]);
        let json = serde_json::to_string(&list).unwrap();
        assert_eq!(json, r#"{"data":[1,2]}"#);
    }

    #[test]
    fn serialize_includes_all_fields_when_present() {
        let list: List<i32> =
            List::new(vec![1]).with_next_cursor("Y3Vy").with_total(7u64);
        let json = serde_json::to_string(&list).unwrap();
        assert_eq!(json, r#"{"data":[1],"next_cursor":"Y3Vy","total":7}"#);
    }

    #[test]
    fn deserialize_round_trip_preserves_fields() {
        let original: List<String> =
            List::new(vec!["a".to_string(), "b".to_string()])
                .with_next_cursor("next")
                .with_total(2u64);
        let json = serde_json::to_string(&original).unwrap();
        let parsed: List<String> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.data, original.data);
        assert_eq!(parsed.next_cursor, original.next_cursor);
        assert_eq!(parsed.total, original.total);
    }

    #[test]
    fn empty_list_serializes_to_empty_data_array() {
        let list: List<i32> = List::default();
        let json = serde_json::to_string(&list).unwrap();
        assert_eq!(json, r#"{"data":[]}"#);
    }

    #[test]
    fn fresh_list_has_no_next_cursor_or_total() {
        // When a handler constructs a fresh List (or calls
        // List::default()), `next_cursor` is `None` and is
        // omitted from the JSON. The spec's "next_cursor: null"
        // scenario is implemented by handlers returning None.
        let list: List<i32> = List::new(vec![1]);
        let json = serde_json::to_string(&list).unwrap();
        assert_eq!(json, r#"{"data":[1]}"#);
        assert!(list.next_cursor.is_none());
    }
}
