//! Storage for full tool outputs that a transformer elided from
//! the conversation.
//!
//! A single tool output can be far larger than what the model's
//! context budget should carry. The steering layer's truncating
//! transformers rewrite such an output into a short excerpt plus a
//! marker naming a **handle**; the full text stays here and the
//! model pulls it back through the `__get_full_output` tool
//! (`synthia_tool::RetrieveFullOutputTool`, which consumes this
//! trait).
//!
//! [`FullOutputStore`] is the seam between the two halves: the
//! transformer depends only on the trait, so a host can swap the
//! in-memory store for a disk-backed or session-scoped one without
//! touching the commit path.
//!
//! # Handle lifetime
//!
//! Handles are **per store, not persistent**. They are minted
//! sequentially (`out-1`, `out-2`, …) and are meaningless outside
//! the store instance that produced them: dropping the store (or
//! restarting the process) loses every handle, and a fresh store
//! starts minting at `out-1` again. Retrieval across a restart
//! requires a [`FullOutputStore`] implementation backed by
//! persistent storage.

use std::{
    collections::VecDeque,
    sync::{Mutex, MutexGuard, PoisonError},
};

/// Backing store for elided tool outputs, keyed by an opaque
/// handle.
///
/// Implementations MUST be thread-safe (`Send + Sync`): one store
/// is shared by the transformer that stashes an output and the tool
/// that serves it back, potentially from different tasks.
pub trait FullOutputStore: Send + Sync {
    /// Stash `text` and return the handle that retrieves it.
    ///
    /// Handles are opaque to callers, but the in-memory
    /// implementation mints them as `out-<n>`, so a marker embedded
    /// in a conversation transcript stays greppable. The returned
    /// handle is the only way back to the text: a caller MUST embed
    /// it in whatever marker it hands to the model.
    #[must_use = "the returned handle is the only way to retrieve the text"]
    fn put(&self, text: String) -> String;

    /// Fetch the text stored under `handle`.
    ///
    /// Returns `None` when the handle is unknown — never minted by
    /// this store, or evicted since.
    #[must_use]
    fn get(&self, handle: &str) -> Option<String>;

    /// Number of entries currently held (not a byte count).
    #[must_use]
    fn len(&self) -> usize;

    /// `true` when the store holds no entries.
    #[must_use]
    fn is_empty(&self) -> bool;
}

/// One stashed output plus the handle that addresses it.
struct Entry {
    handle: String,
    text: String,
}

/// Store state behind the mutex.
struct Inner {
    /// Entries in recency order, least recently used first.
    entries: VecDeque<Entry>,
    /// Next handle ordinal; only ever increases, so a handle is
    /// never reused after its entry is evicted.
    next_id: u64,
    /// Entry ceiling (`None` = unbounded).
    max_entries: Option<usize>,
    /// Total-text ceiling in bytes (`None` = unbounded).
    max_bytes: Option<usize>,
    /// Sum of `Entry::text.len()` over `entries`.
    bytes: usize,
}

impl Inner {
    /// Whether both ceilings currently hold.
    fn within_limits(&self) -> bool {
        let count_ok = self
            .max_entries
            .map(|max| self.entries.len() <= max)
            .unwrap_or(true);
        let bytes_ok =
            self.max_bytes.map(|max| self.bytes <= max).unwrap_or(true);
        count_ok && bytes_ok
    }

    /// Drop least-recently-used entries until both ceilings hold.
    ///
    /// The newest entry is never evicted, so the text `put` just
    /// stashed is always retrievable; a single output larger than
    /// `max_bytes` is kept whole.
    fn evict(&mut self) {
        while self.entries.len() > 1 && !self.within_limits() {
            if let Some(evicted) = self.entries.pop_front() {
                self.bytes -= evicted.text.len();
            }
        }
    }
}

/// In-memory [`FullOutputStore`] with deterministic LRU eviction.
///
/// Two independent ceilings bound the store; the oldest entries are
/// evicted until **both** hold:
///
/// - `max_entries` — how many outputs may be held at once.
/// - `max_bytes` — how many bytes of text may be held in total.
///
/// A zero ceiling means *unbounded* for that dimension, so
/// `InMemoryFullOutputStore::new(0, 0)` is equivalent to
/// [`InMemoryFullOutputStore::unbounded`].
///
/// [`FullOutputStore::get`] counts as a use: the fetched entry
/// becomes the most recently used, so an output the model keeps
/// pulling back survives eviction while untouched ones go first.
///
/// # Example
///
/// ```rust
/// use synthia_core::{FullOutputStore, InMemoryFullOutputStore};
///
/// let store = InMemoryFullOutputStore::new(64, 4 * 1024 * 1024);
/// let handle = store.put("very long output".to_string());
/// assert_eq!(handle, "out-1");
/// assert_eq!(store.get(&handle).as_deref(), Some("very long output"));
/// ```
pub struct InMemoryFullOutputStore {
    inner: Mutex<Inner>,
}

impl InMemoryFullOutputStore {
    /// Store bounded by `max_entries` entries and `max_bytes` bytes
    /// of held text. A zero value disables that ceiling.
    #[must_use]
    pub fn new(max_entries: usize, max_bytes: usize) -> Self {
        Self {
            inner: Mutex::new(Inner {
                entries: VecDeque::new(),
                next_id: 0,
                max_entries: (max_entries > 0).then_some(max_entries),
                max_bytes: (max_bytes > 0).then_some(max_bytes),
                bytes: 0,
            }),
        }
    }

    /// Store with no ceilings — every output ever stashed stays
    /// retrievable until the store is dropped.
    #[must_use]
    pub fn unbounded() -> Self {
        Self::new(0, 0)
    }

    /// Lock the inner state, recovering from poisoning.
    ///
    /// A panic in another thread cannot corrupt the store's
    /// invariants (every mutation is a single `push`/`pop` plus a
    /// byte counter), so a poisoned lock is adopted rather than
    /// propagated: losing the handles would break every marker
    /// already committed to the conversation.
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl FullOutputStore for InMemoryFullOutputStore {
    fn put(&self, text: String) -> String {
        let mut inner = self.lock();
        inner.next_id += 1;
        let handle = format!("out-{}", inner.next_id);
        inner.bytes += text.len();
        inner.entries.push_back(Entry {
            handle: handle.clone(),
            text,
        });
        inner.evict();
        handle
    }

    fn get(&self, handle: &str) -> Option<String> {
        let mut inner = self.lock();
        let position = inner
            .entries
            .iter()
            .position(|entry| entry.handle == handle)?;
        let entry = inner.entries.remove(position)?;
        let text = entry.text.clone();
        inner.entries.push_back(entry);
        Some(text)
    }

    fn len(&self) -> usize {
        self.lock().entries.len()
    }

    fn is_empty(&self) -> bool {
        self.lock().entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stash `text` for tests that assert on eviction rather than on
    /// the returned handle.
    fn stash(store: &InMemoryFullOutputStore, text: &str) {
        let _ = store.put(text.to_string());
    }

    #[test]
    fn put_mints_sequential_handles_and_get_returns_the_text() {
        let store = InMemoryFullOutputStore::unbounded();
        assert!(store.is_empty());
        assert_eq!(store.put("alpha".to_string()), "out-1");
        assert_eq!(store.put("beta".to_string()), "out-2");
        assert_eq!(store.len(), 2);
        assert!(!store.is_empty());
        assert_eq!(store.get("out-1").as_deref(), Some("alpha"));
        assert_eq!(store.get("out-2").as_deref(), Some("beta"));
    }

    #[test]
    fn unknown_handles_resolve_to_none() {
        let store = InMemoryFullOutputStore::unbounded();
        stash(&store, "alpha");
        assert_eq!(store.get("out-2"), None);
        assert_eq!(store.get("nonsense"), None);
        assert_eq!(store.get(""), None);
    }

    #[test]
    fn entry_ceiling_evicts_the_least_recently_used_entry() {
        let store = InMemoryFullOutputStore::new(2, 0);
        stash(&store, "a");
        stash(&store, "b");
        // Touching out-1 makes out-2 the least recently used entry.
        assert_eq!(store.get("out-1").as_deref(), Some("a"));
        stash(&store, "c");
        assert_eq!(store.len(), 2);
        assert_eq!(store.get("out-2"), None);
        assert_eq!(store.get("out-1").as_deref(), Some("a"));
        assert_eq!(store.get("out-3").as_deref(), Some("c"));
    }

    #[test]
    fn entry_ceiling_evicts_the_oldest_entry_when_nothing_was_touched() {
        let store = InMemoryFullOutputStore::new(2, 0);
        stash(&store, "a");
        stash(&store, "b");
        stash(&store, "c");
        assert_eq!(store.len(), 2);
        assert_eq!(store.get("out-1"), None);
        assert_eq!(store.get("out-2").as_deref(), Some("b"));
        assert_eq!(store.get("out-3").as_deref(), Some("c"));
    }

    #[test]
    fn byte_ceiling_evicts_until_the_total_fits() {
        // Ten bytes of ceiling: two five-byte entries fit, the third
        // pushes the total to fifteen, so the oldest goes.
        let store = InMemoryFullOutputStore::new(0, 10);
        stash(&store, "abcde");
        stash(&store, "fghij");
        assert_eq!(store.len(), 2);
        stash(&store, "klmno");
        assert_eq!(store.len(), 2);
        assert_eq!(store.get("out-1"), None);
        assert_eq!(store.get("out-2").as_deref(), Some("fghij"));
        assert_eq!(store.get("out-3").as_deref(), Some("klmno"));
    }

    #[test]
    fn newest_entry_survives_a_byte_ceiling_it_exceeds() {
        let store = InMemoryFullOutputStore::new(0, 4);
        stash(&store, "tiny");
        let handle = store.put("much longer than the ceiling".to_string());
        assert_eq!(store.len(), 1);
        assert_eq!(store.get("out-1"), None);
        assert_eq!(
            store.get(&handle).as_deref(),
            Some("much longer than the ceiling")
        );
    }

    #[test]
    fn handles_are_never_reused_after_eviction() {
        let store = InMemoryFullOutputStore::new(1, 0);
        assert_eq!(store.put("a".to_string()), "out-1");
        assert_eq!(store.put("b".to_string()), "out-2");
        assert_eq!(store.get("out-1"), None);
        assert_eq!(store.get("out-2").as_deref(), Some("b"));
    }

    #[test]
    fn unbounded_store_keeps_every_entry() {
        let store = InMemoryFullOutputStore::unbounded();
        for index in 0..32 {
            let handle = store.put(format!("entry-{index}"));
            assert_eq!(handle, format!("out-{}", index + 1));
        }
        assert_eq!(store.len(), 32);
        assert_eq!(store.get("out-1").as_deref(), Some("entry-0"));
        assert_eq!(store.get("out-32").as_deref(), Some("entry-31"));
    }

    #[test]
    fn entries_are_stored_as_given_even_when_empty() {
        let store = InMemoryFullOutputStore::new(0, 0);
        let handle = store.put(String::new());
        assert_eq!(store.len(), 1);
        assert_eq!(store.get(&handle).as_deref(), Some(""));
    }
}
