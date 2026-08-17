//! Private helper functions shared across route handlers.
//!
//! - [`paginate`]: in-memory keyset pagination over a pre-sorted
//!   `Vec<T>` using the decoded cursor / normalized limit from
//!   [`ResolvedPage`].
//!
//! [`copy_dir_all`] was removed during the 2026-08-15 optimization
//! pass along with `POST /api/v1/skills` (its only consumer).

use crate::api::{List, ResolvedPage, next_cursor};

/// Apply in-memory keyset pagination to a pre-sorted `Vec<T>`.
///
/// The caller MUST sort `items` according to the requested sort
/// field + direction before calling this helper. The helper:
///
/// 1. Records `total = items.len()` (the full matching set size).
/// 2. If `resolved.last_seen_id` is set, finds its position in the
///    sorted list and starts after it. If the cursor ID is not
///    found (deleted resource), returns an empty page (per spec:
///    `"data": [], "next_cursor": null`).
/// 3. Takes `effective_limit` items.
/// 4. If there are more items, encodes the last item's ID into
///    `next_cursor`.
///
/// `id_of` extracts the stable resource identifier (e.g. skill
/// name, task id) used as the cursor payload.
pub(crate) fn paginate<T, F>(
    items: Vec<T>,
    resolved: &ResolvedPage,
    id_of: F,
) -> List<T>
where
    F: Fn(&T) -> &str,
{
    let total = items.len() as u64;
    let limit = resolved.effective_limit as usize;

    let start_idx = match &resolved.last_seen_id {
        None => 0,
        Some(cursor_id) => items
            .iter()
            .position(|item| id_of(item) == cursor_id)
            .map(|p| p + 1)
            .unwrap_or(items.len()),
    };

    if start_idx >= items.len() {
        return List::new(Vec::new()).with_total(total);
    }

    let remaining = items.len() - start_idx;
    let take_count = remaining.min(limit);
    let has_more = remaining > limit;

    let page: Vec<T> =
        items.into_iter().skip(start_idx).take(take_count).collect();

    let last_id = page.last().map(id_of).unwrap_or("");
    let next = next_cursor(last_id, has_more);

    let mut list = List::new(page);
    if let Some(cursor) = next {
        list = list.with_next_cursor(cursor);
    }
    list.with_total(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved(limit: u64, cursor: Option<String>) -> ResolvedPage {
        ResolvedPage {
            last_seen_id: cursor,
            effective_limit: limit,
            sort_field: None,
            descending: false,
        }
    }

    #[test]
    fn paginate_first_page_returns_limit_items() {
        let items: Vec<String> = (0..25).map(|i| format!("item_{i}")).collect();
        let page = paginate(items, &resolved(10, None), |s| s.as_str());
        assert_eq!(page.data.len(), 10);
        assert_eq!(page.total, Some(25));
        assert!(page.next_cursor.is_some());
        assert_eq!(page.data.last().map(|s| s.as_str()), Some("item_9"));
    }

    #[test]
    fn paginate_past_end_returns_empty_and_no_cursor() {
        let items: Vec<String> = (0..25).map(|i| format!("item_{i}")).collect();
        let resolved = ResolvedPage {
            last_seen_id: Some("item_24".to_string()),
            effective_limit: 10,
            sort_field: None,
            descending: false,
        };
        let page = paginate(items, &resolved, |s| s.as_str());
        assert_eq!(page.data.len(), 0);
        assert!(page.next_cursor.is_none());
    }

    #[test]
    fn paginate_last_page_returns_remaining_and_no_cursor() {
        // Cursor at item_19 (the last item of page 2), limit 10:
        // the remaining items are 20-24 (5 items), which fit within
        // the limit, so no `next_cursor` is produced.
        let items: Vec<String> = (0..25).map(|i| format!("item_{i}")).collect();
        let resolved = ResolvedPage {
            last_seen_id: Some("item_19".to_string()),
            effective_limit: 10,
            sort_field: None,
            descending: false,
        };
        let page = paginate(items, &resolved, |s| s.as_str());
        assert_eq!(page.data.len(), 5);
        assert_eq!(page.data.first().map(|s| s.as_str()), Some("item_20"));
        assert_eq!(page.data.last().map(|s| s.as_str()), Some("item_24"));
        assert!(page.next_cursor.is_none());
        assert_eq!(page.total, Some(25));
    }

    #[test]
    fn paginate_cursor_in_middle_resumes_correctly() {
        let items: Vec<String> = (0..25).map(|i| format!("item_{i}")).collect();
        let resolved = ResolvedPage {
            last_seen_id: Some("item_9".to_string()),
            effective_limit: 10,
            sort_field: None,
            descending: false,
        };
        let page = paginate(items, &resolved, |s| s.as_str());
        assert_eq!(page.data.len(), 10);
        assert_eq!(page.data.first().map(|s| s.as_str()), Some("item_10"));
        assert_eq!(page.data.last().map(|s| s.as_str()), Some("item_19"));
        assert!(page.next_cursor.is_some());
    }

    #[test]
    fn paginate_deleted_cursor_returns_empty_page() {
        let items: Vec<String> = (0..10).map(|i| format!("item_{i}")).collect();
        let resolved = ResolvedPage {
            last_seen_id: Some("deleted_id".to_string()),
            effective_limit: 10,
            sort_field: None,
            descending: false,
        };
        let page = paginate(items, &resolved, |s| s.as_str());
        assert_eq!(page.data.len(), 0);
        assert!(page.next_cursor.is_none());
        assert_eq!(page.total, Some(10));
    }

    #[test]
    fn paginate_fewer_than_limit_returns_all_no_cursor() {
        let items: Vec<String> = (0..5).map(|i| format!("item_{i}")).collect();
        let page = paginate(items, &resolved(10, None), |s| s.as_str());
        assert_eq!(page.data.len(), 5);
        assert!(page.next_cursor.is_none());
    }

    #[test]
    fn paginate_exactly_limit_returns_no_cursor() {
        let items: Vec<String> = (0..10).map(|i| format!("item_{i}")).collect();
        let page = paginate(items, &resolved(10, None), |s| s.as_str());
        assert_eq!(page.data.len(), 10);
        assert!(page.next_cursor.is_none());
    }
}
