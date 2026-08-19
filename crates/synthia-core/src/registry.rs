//! Registry trait definition.
//!
//! # Design
//!
//! A **registry** is a catalog of named items plus their metadata
//! entries. [`Registry`] is a single trait that covers registration
//! (`put` / `delete`), querying (`get`), cursor-paginated listing
//! ([`Registry::list_paginate`]), and an unpaginated convenience
//! ([`Registry::list`], defaulted). The trait uses two associated
//! types:
//!
//! - [`Registry::Item`] — the value the registry stores and returns.
//!   It implements [`RegistryItem`] (giving `name()` / `description()`)
//!   and is `Clone` so the trait can hand copies back to callers.
//! - [`Registry::Filter`] — the list filter type. Use [`EmptyFilter`]
//!   when no filtering is needed.
//!
//! # Listing model
//!
//! Registries expose two listing-shaped methods:
//!
//! - [`Registry::list_paginate`] returns a [`RegistryList<T>`]
//!   envelope (`data`, `next_cursor`, `total`) with cursor-based
//!   keyset pagination. Implementations fetch their full filtered
//!   set themselves and then delegate the slicing/encoding work to
//!   [`paginate_registry_list`].
//! - [`Registry::list`] has a default implementation that calls
//!   [`Registry::list_paginate`] with `limit = u64::MAX` and returns
//!   just the `data` vector. Registries that have a more efficient
//!   "everything" path may override it.
//!
//! # No `Serialize` / `Deserialize` bound
//!
//! `Registry::Item` is intentionally not required to be
//! `(de)serializable`. Most `Item` values are constructed at runtime
//! from in-memory state and cannot be (de)serialized in any
//! meaningful sense. Persistence is a per-implementation concern; it
//! lives on impl-side helper methods, not on the trait contract.

use async_trait::async_trait;

use crate::error::Error;

/// Maximum page size enforced by [`paginate_registry_list`]. Larger
/// `limit` values are silently truncated to this value.
///
/// Mirrors the wire-level constant in `synthia_server::api::v1`
/// — the two MUST stay in sync so the cursor envelope emitted by
/// `synthia-server` handlers agrees with what `Registry`'s default
/// cursor decoder produces.
const REGISTRY_MAX_LIMIT: u64 = 100;

/// Lightweight cursor-list envelope returned by
/// [`Registry::list_paginate`].
///
/// This is a mirror of the wire-level
/// `synthia_server::api::v1::List<T>` — same shape, same JSON
/// serialization. We keep an independent copy in core because the
/// registry pagination logic is generic over `Self::Item` and must
/// not depend on the `synthia-server` wire envelope.
///
/// Public so external crates (synthia-agent, synthia-tool, …)
/// that call `Registry::list_paginate` directly can read the
/// fields. `synthia-server` routes wrap the result in their own
/// `api::v1::List<T>` for the wire response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryList<T> {
    pub data: Vec<T>,
    pub next_cursor: Option<String>,
    pub total: Option<u64>,
}

impl<T> RegistryList<T> {
    pub fn new(data: Vec<T>) -> Self {
        Self {
            data,
            next_cursor: None,
            total: None,
        }
    }

    pub fn with_next_cursor(mut self, cursor: impl Into<String>) -> Self {
        self.next_cursor = Some(cursor.into());
        self
    }

    pub fn with_total(mut self, total: u64) -> Self {
        self.total = Some(total);
        self
    }
}

/// Decode a registry cursor into the underlying resource ID.
///
/// The encoding MUST match `synthia_server::api::v1::decode_cursor`
/// (URL-safe base64, no padding, UTF-8 validated). Returns
/// `Err(Error::invalid_item)` on a malformed cursor so callers
/// surface the standard 400 wire code.
fn decode_registry_cursor(cursor: &str) -> Result<String, Error> {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor.as_bytes())
        .map_err(|_| Error::invalid_item("cursor"))?;
    String::from_utf8(bytes).map_err(|_| Error::invalid_item("cursor"))
}

/// Encode the last resource ID into an opaque cursor. Mirror of
/// `synthia_server::api::v1::encode_cursor`.
fn encode_registry_cursor(id: &str) -> String {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    URL_SAFE_NO_PAD.encode(id.as_bytes())
}

/// Slice a fully-fetched `items` vector into the page described by
/// the cursor + limit, returning a [`RegistryList<T>`] envelope.
///
/// This is the shared pagination primitive every
/// [`Registry::list_paginate`] implementation should delegate to:
/// each registry is responsible only for fetching its filtered set
/// (which requires knowledge of the underlying store), and then
/// hands the result here for the cursor arithmetic + envelope
/// construction.
///
/// # Cursor
///
/// The cursor is the opaque base64 encoding of the last
/// resource's [`RegistryItem::name`] on the previous page.
/// Decode failures return [`Error::InvalidItem`]. A cursor that
/// points to a deleted resource yields an empty page (per spec:
/// `data: [], next_cursor: null`). An empty-string cursor is
/// treated as "no cursor" (first page).
///
/// # Limit
///
/// `limit == 0` returns [`Error::Validation`]. `limit` above
/// [`REGISTRY_MAX_LIMIT`] is silently truncated to
/// [`REGISTRY_MAX_LIMIT`].
///
/// Callers that don't need pagination can pass `limit = u64::MAX`
/// — they get the full set back with `total` populated, or call
/// [`Registry::list`] for the unpaginated convenience.
pub fn paginate_registry_list<T: RegistryItem>(
    items: Vec<T>,
    cursor: Option<&str>,
    limit: u64,
) -> Result<RegistryList<T>, Error> {
    let total = items.len() as u64;

    let effective_limit = if limit == 0 {
        return Err(Error::validation("limit must be greater than 0"));
    } else {
        limit.min(REGISTRY_MAX_LIMIT)
    };

    let last_seen_id: Option<String> = match cursor {
        None => None,
        Some("") => None,
        Some(c) => Some(decode_registry_cursor(c)?),
    };

    let start_idx: usize = match &last_seen_id {
        None => 0,
        Some(cursor_id) => items
            .iter()
            .position(|item| item.name() == cursor_id.as_str())
            .map(|p| p + 1)
            .unwrap_or(items.len()),
    };

    if start_idx >= items.len() {
        return Ok(RegistryList::new(Vec::new()).with_total(total));
    }

    let remaining = items.len() - start_idx;
    let limit_usize = effective_limit as usize;
    let take_count = remaining.min(limit_usize);
    let has_more = remaining > limit_usize;

    let page: Vec<T> =
        items.into_iter().skip(start_idx).take(take_count).collect();

    let last_id = page.last().map(|item| item.name()).unwrap_or("");
    let next = if has_more {
        Some(encode_registry_cursor(last_id))
    } else {
        None
    };

    let mut list = RegistryList::new(page);
    if let Some(c) = next {
        list = list.with_next_cursor(c);
    }
    Ok(list.with_total(total))
}

/// No-op filter. Use as [`Registry::Filter`] when the registry has
/// no filterable fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EmptyFilter;

/// Metadata contract for anything stored in a [`Registry`].
///
/// Every `Registry::Item` implements this so the registry can
/// extract a uniform `name()` + `description()` for cursor
/// encoding, listing, and the `paginate_registry_list` helper
/// without knowing the item's concrete type.
pub trait RegistryItem: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
}

/// Named-item catalog trait.
///
/// Each implementor declares the item type it stores
/// ([`Registry::Item`]) and the list filter it supports
/// ([`Registry::Filter`]). Implementations of [`Registry::list_paginate`]
/// should fetch the full filtered set themselves and delegate the
/// cursor/limit/envelope construction to [`paginate_registry_list`].
///
/// [`Registry::list`] has a default implementation that calls
/// [`Registry::list_paginate`] with `limit = u64::MAX` and returns
/// the resulting `data` vector.
#[async_trait]
pub trait Registry: Send + Sync {
    type Item: RegistryItem + Clone + Send + Sync + 'static;
    type Filter: Clone + Send + Sync + 'static;

    /// Insert (or replace) an item by its [`RegistryItem::name`].
    /// Returns an error if the registration is rejected
    /// (e.g. duplicate name, missing dependency).
    async fn put(&self, item: Self::Item) -> Result<(), Error>;

    /// Remove an item by metadata name. Returns
    /// `Err(Error::NotFound)` if no item with that name is currently
    /// registered.
    async fn delete(&self, name: &str) -> Result<(), Error>;

    /// Look up a single metadata entry by name.
    async fn get(&self, name: &str) -> Result<Option<Self::Item>, Error>;

    /// Paginated list with cursor-based keyset pagination.
    ///
    /// Callers that don't need pagination should call
    /// [`Registry::list`] instead. The `sort` parameter is accepted
    /// for API compatibility; implementations are free to apply it
    /// or ignore it (in-memory registries typically ignore it and
    /// return insertion order).
    ///
    /// Cursor semantics, `limit` validation, and the
    /// `data`/`next_cursor`/`total` envelope are all handled by
    /// [`paginate_registry_list`] — implementations are expected to
    /// fetch their filtered set and then delegate.
    async fn list_paginate(
        &self,
        cursor: Option<String>,
        limit: u64,
        sort: Option<String>,
        filter: Option<Self::Filter>,
    ) -> Result<RegistryList<Self::Item>, Error>;

    /// Unpaginated list. Returns the full filtered `data` vector of
    /// [`Registry::list_paginate`] with `limit = u64::MAX`.
    ///
    /// Equivalent to
    /// `self.list_paginate(None, u64::MAX, None, filter).await?.data`.
    /// Override only if the implementation has a more efficient
    /// "everything" path.
    async fn list(
        &self,
        filter: Option<Self::Filter>,
    ) -> Result<Vec<Self::Item>, Error> {
        let page = self.list_paginate(None, u64::MAX, None, filter).await?;
        Ok(page.data)
    }

    /// Begin the registry's lifecycle for the named item. The
    /// default is a no-op returning `Ok(())`, suitable for stateless
    /// or fully-in-memory registries (e.g. `ToolRegistry`,
    /// `AgentRegistry`).
    ///
    /// Implementors that own background work, file handles, or
    /// network resources associated with a named item MUST override
    /// this hook. The name is the [`RegistryItem::name`] of the
    /// entry to start.
    async fn start(&self, _name: &str) -> Result<(), Error> {
        Ok(())
    }

    /// End the registry's lifecycle for the named item. The default
    /// is a no-op returning `Ok(())`, symmetric to
    /// [`Registry::start`]. Override only when an implementor
    /// actually holds lifecycle state to release.
    async fn stop(&self, _name: &str) -> Result<(), Error> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use parking_lot::RwLock;

    use super::*;
    use crate::error::Error;

    /// A minimal item used by the mock registry.
    #[derive(Clone, Debug, PartialEq)]
    struct MockItem {
        name: String,
        description: String,
    }

    impl MockItem {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                description: format!("desc for {name}"),
            }
        }
    }

    impl RegistryItem for MockItem {
        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            &self.description
        }
    }

    /// An in-memory mock registry backed by a `Vec` to preserve
    /// insertion order (so tests have a stable, predictable ordering
    /// without relying on the default impl's sort).
    struct MockRegistry {
        items: RwLock<Vec<MockItem>>,
    }

    impl MockRegistry {
        fn new(items: Vec<MockItem>) -> Self {
            Self {
                items: RwLock::new(items),
            }
        }
    }

    #[async_trait]
    impl Registry for MockRegistry {
        type Filter = EmptyFilter;
        type Item = MockItem;

        async fn put(&self, item: MockItem) -> Result<(), Error> {
            let mut items = self.items.write();
            if items.iter().any(|i| i.name == item.name) {
                return Err(Error::already_exists(item.name.clone()));
            }
            items.push(item);
            Ok(())
        }

        async fn delete(&self, name: &str) -> Result<(), Error> {
            let mut items = self.items.write();
            let before = items.len();
            items.retain(|i| i.name != name);
            if items.len() == before {
                return Err(Error::not_found(name));
            }
            Ok(())
        }

        async fn get(&self, name: &str) -> Result<Option<MockItem>, Error> {
            Ok(self.items.read().iter().find(|i| i.name == name).cloned())
        }

        async fn list_paginate(
            &self,
            cursor: Option<String>,
            limit: u64,
            _sort: Option<String>,
            _filter: Option<Self::Filter>,
        ) -> Result<RegistryList<MockItem>, Error> {
            let items = self.items.read().clone();
            // Sort is intentionally ignored — tests rely on
            // insertion order for stable assertions.
            paginate_registry_list(items, cursor.as_deref(), limit)
        }
    }

    fn items_named(names: &[&str]) -> Vec<MockItem> {
        names.iter().map(|n| MockItem::new(n)).collect()
    }

    // --- put / delete / get ---

    #[tokio::test]
    async fn put_inserts_and_get_returns_clone() {
        let reg = MockRegistry::new(Vec::new());
        reg.put(MockItem::new("a")).await.unwrap();
        let got = reg.get("a").await.unwrap();
        assert_eq!(got, Some(MockItem::new("a")));
    }

    #[tokio::test]
    async fn put_duplicate_returns_error() {
        let reg = MockRegistry::new(items_named(&["a"]));
        let err = reg.put(MockItem::new("a")).await.unwrap_err();
        assert!(matches!(err, Error::AlreadyExists { .. }), "got {err:?}");
    }

    #[tokio::test]
    async fn delete_removes_and_unknown_returns_error() {
        let reg = MockRegistry::new(items_named(&["a", "b"]));
        reg.delete("a").await.unwrap();
        assert!(reg.get("a").await.unwrap().is_none());
        let err = reg.delete("nope").await.unwrap_err();
        assert!(matches!(err, Error::NotFound { .. }), "got {err:?}");
    }

    // --- first page ---

    #[tokio::test]
    async fn first_page_returns_limit_items_with_cursor() {
        let reg = MockRegistry::new(items_named(&["a", "b", "c", "d", "e"]));
        let page = reg.list_paginate(None, 2, None, None).await.unwrap();
        assert_eq!(page.data.len(), 2);
        assert_eq!(page.data[0].name, "a");
        assert_eq!(page.data[1].name, "b");
        assert!(page.next_cursor.is_some());
        assert_eq!(page.total, Some(5));
    }

    // --- middle page ---

    #[tokio::test]
    async fn middle_page_resumes_after_cursor_id() {
        let reg = MockRegistry::new(items_named(&["a", "b", "c", "d", "e"]));
        // Cursor encodes "b" → resume at "c"
        let cursor = encode_registry_cursor("b");
        let page = reg
            .list_paginate(Some(cursor), 2, None, None)
            .await
            .unwrap();
        assert_eq!(page.data.len(), 2);
        assert_eq!(page.data[0].name, "c");
        assert_eq!(page.data[1].name, "d");
        assert!(page.next_cursor.is_some());
        // next_cursor should encode "d"
        let next_id =
            decode_registry_cursor(page.next_cursor.as_deref().unwrap())
                .unwrap();
        assert_eq!(next_id, "d");
        assert_eq!(page.total, Some(5));
    }

    // --- last page ---

    #[tokio::test]
    async fn last_page_returns_remaining_with_no_cursor() {
        let reg = MockRegistry::new(items_named(&["a", "b", "c", "d", "e"]));
        // Cursor encodes "d" → resume at "e" (only 1 item left)
        let cursor = encode_registry_cursor("d");
        let page = reg
            .list_paginate(Some(cursor), 10, None, None)
            .await
            .unwrap();
        assert_eq!(page.data.len(), 1);
        assert_eq!(page.data[0].name, "e");
        assert!(page.next_cursor.is_none());
        assert_eq!(page.total, Some(5));
    }

    // --- past-end (cursor at last item, no more data) ---

    #[tokio::test]
    async fn cursor_at_last_item_returns_empty_page() {
        let reg = MockRegistry::new(items_named(&["a", "b", "c", "d", "e"]));
        // Cursor encodes "e" (the last item) → start_idx == len → empty
        let cursor = encode_registry_cursor("e");
        let page = reg
            .list_paginate(Some(cursor), 10, None, None)
            .await
            .unwrap();
        assert_eq!(page.data.len(), 0);
        assert!(page.next_cursor.is_none());
        assert_eq!(page.total, Some(5));
    }

    // --- deleted cursor (cursor ID no longer in list) ---

    #[tokio::test]
    async fn deleted_cursor_returns_empty_page_not_error() {
        let reg = MockRegistry::new(items_named(&["a", "b", "c"]));
        // Cursor points to an ID that doesn't exist (deleted)
        let cursor = encode_registry_cursor("deleted_id");
        let page = reg
            .list_paginate(Some(cursor), 10, None, None)
            .await
            .unwrap();
        assert_eq!(page.data.len(), 0);
        assert!(page.next_cursor.is_none());
        assert_eq!(page.total, Some(3));
    }

    // --- invalid cursor (bad base64) ---

    #[tokio::test]
    async fn invalid_cursor_returns_invalid_item_error() {
        let reg = MockRegistry::new(items_named(&["a", "b", "c"]));
        let err = reg
            .list_paginate(
                Some("not-valid-base64!!!".to_string()),
                10,
                None,
                None,
            )
            .await
            .unwrap_err();
        match err {
            Error::InvalidItem { item, .. } => {
                assert_eq!(item, "cursor");
            }
            other => panic!("expected Error::InvalidItem, got {other:?}"),
        }
    }

    // --- empty-string cursor treated as first page ---

    #[tokio::test]
    async fn empty_string_cursor_treated_as_first_page() {
        let reg = MockRegistry::new(items_named(&["a", "b", "c"]));
        let page = reg
            .list_paginate(Some(String::new()), 2, None, None)
            .await
            .unwrap();
        assert_eq!(page.data.len(), 2);
        assert_eq!(page.data[0].name, "a");
        assert!(page.next_cursor.is_some());
    }

    // --- empty registry ---

    #[tokio::test]
    async fn empty_registry_returns_empty_page_no_cursor() {
        let reg = MockRegistry::new(Vec::new());
        let page = reg.list_paginate(None, 10, None, None).await.unwrap();
        assert_eq!(page.data.len(), 0);
        assert!(page.next_cursor.is_none());
        assert_eq!(page.total, Some(0));
    }

    // --- limit boundaries ---

    #[tokio::test]
    async fn limit_zero_returns_validation_error() {
        let reg = MockRegistry::new(items_named(&["a", "b", "c"]));
        let err = reg.list_paginate(None, 0, None, None).await.unwrap_err();
        match err {
            Error::Validation { message, .. } => {
                assert!(message.contains("limit"), "message = {message}");
            }
            other => panic!("expected Error::Validation, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn limit_above_max_is_truncated() {
        // Build a registry with REGISTRY_MAX_LIMIT + 5 items, ask
        // for a page larger than REGISTRY_MAX_LIMIT, expect exactly
        // REGISTRY_MAX_LIMIT back.
        let names: Vec<String> = (0..(REGISTRY_MAX_LIMIT + 5))
            .map(|i| format!("item_{i:03}"))
            .collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let reg = MockRegistry::new(items_named(&refs));
        let page = reg
            .list_paginate(None, REGISTRY_MAX_LIMIT + 50, None, None)
            .await
            .unwrap();
        assert_eq!(page.data.len() as u64, REGISTRY_MAX_LIMIT);
        assert!(page.next_cursor.is_some());
        assert_eq!(page.total, Some(REGISTRY_MAX_LIMIT + 5));
    }

    #[tokio::test]
    async fn exactly_limit_items_yields_no_cursor() {
        let reg = MockRegistry::new(items_named(&["a", "b", "c"]));
        let page = reg.list_paginate(None, 3, None, None).await.unwrap();
        assert_eq!(page.data.len(), 3);
        assert!(page.next_cursor.is_none());
    }

    #[tokio::test]
    async fn one_more_than_limit_yields_cursor() {
        let reg = MockRegistry::new(items_named(&["a", "b", "c", "d"]));
        let page = reg.list_paginate(None, 3, None, None).await.unwrap();
        assert_eq!(page.data.len(), 3);
        assert!(page.next_cursor.is_some());
        let next_id =
            decode_registry_cursor(page.next_cursor.as_deref().unwrap())
                .unwrap();
        assert_eq!(next_id, "c");
    }

    // --- walk a 5-item list page-by-page ---

    #[tokio::test]
    async fn walk_three_pages_then_terminate() {
        let reg = MockRegistry::new(items_named(&["a", "b", "c", "d", "e"]));

        // Page 1: a, b → next_cursor encodes "b"
        let p1 = reg.list_paginate(None, 2, None, None).await.unwrap();
        assert_eq!(
            p1.data.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(),
            vec!["a", "b"]
        );
        assert!(p1.next_cursor.is_some());

        // Page 2: c, d → next_cursor encodes "d"
        let p2 = reg
            .list_paginate(p1.next_cursor, 2, None, None)
            .await
            .unwrap();
        assert_eq!(
            p2.data.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(),
            vec!["c", "d"]
        );
        assert!(p2.next_cursor.is_some());

        // Page 3: e → no next_cursor
        let p3 = reg
            .list_paginate(p2.next_cursor, 2, None, None)
            .await
            .unwrap();
        assert_eq!(
            p3.data.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(),
            vec!["e"]
        );
        assert!(p3.next_cursor.is_none());
    }

    // --- sort parameter is ignored by default impl ---

    #[tokio::test]
    async fn sort_parameter_is_ignored_by_default_impl() {
        let reg = MockRegistry::new(items_named(&["b", "a", "c"]));
        // Default impl should return items in insertion order
        // regardless of the sort parameter.
        let page = reg
            .list_paginate(None, 10, Some("-name".to_string()), None)
            .await
            .unwrap();
        assert_eq!(
            page.data
                .iter()
                .map(|i| i.name.as_str())
                .collect::<Vec<_>>(),
            vec!["b", "a", "c"]
        );
    }

    // --- list default impl ---

    #[tokio::test]
    async fn list_default_returns_full_data_without_pagination() {
        let reg = MockRegistry::new(items_named(&["a", "b", "c", "d", "e"]));
        let all = reg.list(None).await.unwrap();
        assert_eq!(
            all.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(),
            vec!["a", "b", "c", "d", "e"]
        );
    }

    #[tokio::test]
    async fn list_default_on_empty_registry_returns_empty_vec() {
        let reg = MockRegistry::new(Vec::new());
        let all = reg.list(None).await.unwrap();
        assert!(all.is_empty());
    }

    // --- default start / stop lifecycle hooks ---

    #[tokio::test]
    async fn start_default_is_noop_returning_ok_for_lifecycle_free_registry() {
        // MockRegistry does NOT override `start`; the trait default
        // must be inherited and return Ok(()) for any name.
        let reg = MockRegistry::new(Vec::new());
        let result = Registry::start(&reg, "anything").await;
        assert!(result.is_ok(), "expected Ok(()), got {result:?}");
    }

    #[tokio::test]
    async fn stop_default_is_noop_returning_ok_for_lifecycle_free_registry() {
        // MockRegistry does NOT override `stop`; the trait default
        // must be inherited and return Ok(()) for any name.
        let reg = MockRegistry::new(Vec::new());
        let result = Registry::stop(&reg, "anything").await;
        assert!(result.is_ok(), "expected Ok(()), got {result:?}");
    }
}
