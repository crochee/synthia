//! Opaque cursor encoding for keyset pagination.
//!
//! Cursor = base64 (URL-safe, no-pad) encoding of the last
//! resource ID on the current page. Clients treat it as opaque;
//! servers decode it to resume via `WHERE id > last_id`.
//!
//! These helpers return [`crate::error::Error`] (the
//! transport-agnostic core error). HTTP transports can then
//! map specific variants (`Error::InvalidItem`,
//! `Error::Validation`) to the appropriate wire envelope
//! without the cursor helpers having to know the transport.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use synthia_core::Error;

use super::page_query::{DEFAULT_LIMIT, MAX_LIMIT, PageQuery};

/// Encode a resource ID into an opaque base64 cursor.
///
/// ```
/// use synthia_server::api::encode_cursor;
/// assert_eq!(encode_cursor("task_abc"), "dGFza19hYmM");
/// ```
pub fn encode_cursor(id: &str) -> String {
    URL_SAFE_NO_PAD.encode(id.as_bytes())
}

/// Decode an opaque base64 cursor back into a resource ID.
///
/// Returns [`Error`] with the [`Error::InvalidItem`] variant if
/// the input is not valid URL-safe base64 or is not valid
/// UTF-8 after decoding.
pub fn decode_cursor(cursor: &str) -> Result<String, Error> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor.as_bytes())
        .map_err(|_| Error::invalid_item("cursor"))?;
    String::from_utf8(bytes).map_err(|_| Error::invalid_item("cursor"))
}

/// Build a `next_cursor` value for a list response.
///
/// Returns `Some(encode_cursor(last_item_id))` when `has_more`
/// is true, otherwise `None`. Handlers call this after slicing a
/// page to decide whether to populate `List::next_cursor`.
pub fn next_cursor(last_item_id: &str, has_more: bool) -> Option<String> {
    if has_more {
        Some(encode_cursor(last_item_id))
    } else {
        None
    }
}

/// Parsed sort parameter: `(field, descending)`.
///
/// - `None` → `(None, false)` (use resource default).
/// - `Some("field")` → `(Some("field"), false)`.
/// - `Some("-field")` → `(Some("field"), true)`.
///
/// Field-level whitelist validation is the caller's
/// responsibility — use [`super::validation::validate_sort`]
/// after parsing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedSort {
    pub field: Option<String>,
    pub descending: bool,
}

/// Parse a sort parameter into [`ParsedSort`].
///
/// Does NOT validate against any whitelist. Callers should
/// follow up with [`super::validation::validate_sort`] (or
/// build the whitelist check into their own handler).
pub fn parse_sort(sort: &Option<String>) -> ParsedSort {
    match sort {
        None => ParsedSort {
            field: None,
            descending: false,
        },
        Some(s) => {
            let s = s.trim();
            if let Some(stripped) = s.strip_prefix('-') {
                if stripped.is_empty() {
                    ParsedSort {
                        field: None,
                        descending: true,
                    }
                } else {
                    ParsedSort {
                        field: Some(stripped.to_string()),
                        descending: true,
                    }
                }
            } else if s.is_empty() {
                ParsedSort {
                    field: None,
                    descending: false,
                }
            } else {
                ParsedSort {
                    field: Some(s.to_string()),
                    descending: false,
                }
            }
        }
    }
}

/// Normalize a raw `limit` parameter.
///
/// - `None` → `DEFAULT_LIMIT` (20).
/// - `Some(0)` → `Err(Error)` with the
///   [`Error::Validation`] variant.
/// - `Some(n)` where `n > MAX_LIMIT` → `MAX_LIMIT` (100),
///   silently truncated.
/// - Otherwise → `n`.
pub fn normalize_limit(limit: Option<u64>) -> Result<u64, Error> {
    match limit {
        None => Ok(DEFAULT_LIMIT),
        Some(0) => Err(Error::validation("limit must be greater than 0")),
        Some(n) if n > MAX_LIMIT => Ok(MAX_LIMIT),
        Some(n) => Ok(n),
    }
}

/// Fully-resolved page parameters after decoding / normalizing.
///
/// Produced by [`resolve_page`]. Handlers receive this and can
/// run a single `WHERE id > last_seen_id ORDER BY field
/// (ASC|DESC) LIMIT effective_limit+1` query.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResolvedPage {
    /// Decoded cursor — the last resource ID seen on the
    /// previous page. `None` for the first page.
    pub last_seen_id: Option<String>,
    /// Normalized limit (1..=MAX_LIMIT).
    pub effective_limit: u64,
    /// Parsed sort field (no `-` prefix). `None` means "use
    /// resource default sort".
    pub sort_field: Option<String>,
    /// `true` if the sort had a `-` prefix.
    pub descending: bool,
}

/// Resolve a [`PageQuery`] into a fully-decoded, normalized
/// [`ResolvedPage`].
///
/// This composes [`decode_cursor`], [`normalize_limit`], and
/// [`parse_sort`]. It does NOT validate the sort field against
/// any whitelist — handlers should call
/// [`super::validation::validate_sort`] after resolving, using
/// their resource-specific whitelist.
///
/// # Errors
///
/// - `Err(Error::InvalidItem)` if `cursor` is set but cannot
///   be decoded.
/// - `Err(Error::Validation)` if `limit == 0`.
pub fn resolve_page(query: &PageQuery) -> Result<ResolvedPage, Error> {
    let last_seen_id = match query.cursor.as_deref() {
        None => None,
        Some("") => None,
        Some(c) => Some(decode_cursor(c)?),
    };
    let effective_limit = normalize_limit(query.limit)?;
    let parsed = parse_sort(&query.sort);
    Ok(ResolvedPage {
        last_seen_id,
        effective_limit,
        sort_field: parsed.field,
        descending: parsed.descending,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- encode_cursor / decode_cursor ---

    #[test]
    fn encode_cursor_matches_spec_example() {
        // Spec: cursor of "task_abc" = "dGFza19hYmM"
        // (base64 URL-safe no-pad).
        assert_eq!(encode_cursor("task_abc"), "dGFza19hYmM");
    }

    #[test]
    fn decode_cursor_round_trip() {
        let id = "task_abc";
        let encoded = encode_cursor(id);
        let decoded = decode_cursor(&encoded).unwrap();
        assert_eq!(decoded, id);
    }

    #[test]
    fn decode_cursor_handles_unicode_ids() {
        let id = "task_αβγ";
        let encoded = encode_cursor(id);
        let decoded = decode_cursor(&encoded).unwrap();
        assert_eq!(decoded, id);
    }

    #[test]
    fn decode_cursor_rejects_invalid_base64() {
        assert!(decode_cursor("not-base64!!!").is_err());
    }

    #[test]
    fn decode_cursor_rejects_non_utf8_payload() {
        // 0xff is not valid UTF-8 (standalone).
        let bad = URL_SAFE_NO_PAD.encode([0xffu8]);
        assert!(decode_cursor(&bad).is_err());
    }

    // --- next_cursor ---

    #[test]
    fn next_cursor_some_when_has_more() {
        let c = next_cursor("task_abc", true);
        assert_eq!(c.as_deref(), Some("dGFza19hYmM"));
    }

    #[test]
    fn next_cursor_none_when_no_more() {
        assert_eq!(next_cursor("task_abc", false), None);
    }

    // --- parse_sort ---

    #[test]
    fn parse_sort_none_yields_empty_default() {
        let p = parse_sort(&None);
        assert!(p.field.is_none());
        assert!(!p.descending);
    }

    #[test]
    fn parse_sort_plain_field_is_ascending() {
        let p = parse_sort(&Some("created_at".to_string()));
        assert_eq!(p.field.as_deref(), Some("created_at"));
        assert!(!p.descending);
    }

    #[test]
    fn parse_sort_minus_prefix_is_descending() {
        let p = parse_sort(&Some("-created_at".to_string()));
        assert_eq!(p.field.as_deref(), Some("created_at"));
        assert!(p.descending);
    }

    #[test]
    fn parse_sort_minus_only_yields_no_field_descending() {
        let p = parse_sort(&Some("-".to_string()));
        assert!(p.field.is_none());
        assert!(p.descending);
    }

    #[test]
    fn parse_sort_trims_whitespace() {
        let p = parse_sort(&Some("  -created_at  ".to_string()));
        assert_eq!(p.field.as_deref(), Some("created_at"));
        assert!(p.descending);
    }

    // --- normalize_limit ---

    #[test]
    fn normalize_limit_none_yields_default() {
        assert_eq!(normalize_limit(None).unwrap(), DEFAULT_LIMIT);
    }

    #[test]
    fn normalize_limit_zero_is_error() {
        assert!(normalize_limit(Some(0)).is_err());
    }

    #[test]
    fn normalize_limit_within_range_is_passthrough() {
        assert_eq!(normalize_limit(Some(1)).unwrap(), 1);
        assert_eq!(normalize_limit(Some(20)).unwrap(), 20);
        assert_eq!(normalize_limit(Some(100)).unwrap(), 100);
    }

    #[test]
    fn normalize_limit_above_max_is_truncated_to_max() {
        assert_eq!(normalize_limit(Some(101)).unwrap(), MAX_LIMIT);
        assert_eq!(normalize_limit(Some(1000)).unwrap(), MAX_LIMIT);
    }

    // --- resolve_page ---

    #[test]
    fn resolve_page_first_page_default() {
        let q = PageQuery::default();
        let r = resolve_page(&q).unwrap();
        assert!(r.last_seen_id.is_none());
        assert_eq!(r.effective_limit, DEFAULT_LIMIT);
        assert!(r.sort_field.is_none());
        assert!(!r.descending);
    }

    #[test]
    fn resolve_page_with_cursor_decodes_id() {
        let q = PageQuery::new().with_cursor(encode_cursor("task_xyz"));
        let r = resolve_page(&q).unwrap();
        assert_eq!(r.last_seen_id.as_deref(), Some("task_xyz"));
    }

    #[test]
    fn resolve_page_empty_cursor_string_treated_as_first_page() {
        // Empty cursor → first page (don't error on empty string).
        let q = PageQuery::new().with_cursor("");
        let r = resolve_page(&q).unwrap();
        assert!(r.last_seen_id.is_none());
    }

    #[test]
    fn resolve_page_invalid_cursor_errors() {
        let q = PageQuery::new().with_cursor("not-base64!!!");
        assert!(resolve_page(&q).is_err());
    }

    #[test]
    fn resolve_page_zero_limit_errors() {
        let q = PageQuery::new().with_limit(0u64);
        assert!(resolve_page(&q).is_err());
    }

    #[test]
    fn resolve_page_limit_above_max_truncated() {
        let q = PageQuery::new().with_limit(500u64);
        let r = resolve_page(&q).unwrap();
        assert_eq!(r.effective_limit, MAX_LIMIT);
    }

    #[test]
    fn resolve_page_sort_parsed() {
        let q = PageQuery::new().with_sort("-created_at");
        let r = resolve_page(&q).unwrap();
        assert_eq!(r.sort_field.as_deref(), Some("created_at"));
        assert!(r.descending);
    }

    #[test]
    fn resolve_page_combined() {
        let q = PageQuery::new()
            .with_cursor(encode_cursor("task_xyz"))
            .with_limit(50u64)
            .with_sort("-created_at");
        let r = resolve_page(&q).unwrap();
        assert_eq!(r.last_seen_id.as_deref(), Some("task_xyz"));
        assert_eq!(r.effective_limit, 50);
        assert_eq!(r.sort_field.as_deref(), Some("created_at"));
        assert!(r.descending);
    }
}
