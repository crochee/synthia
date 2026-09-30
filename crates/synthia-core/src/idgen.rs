//! Identifier generation abstraction.
//!
//! Production code stamps session IDs, tool-call IDs, and event
//! IDs with a generator trait so tests can return a deterministic
//! sequence and so a host can plug in a custom scheme (UUIDv7,
//! KSUID, prefixed ULID, …) without touching the call sites.
//!
//! # Why a trait, not a free function
//!
//! The Synthia runtime hands `Arc<dyn IdGen>` to pieces that
//! mint IDs (`ReActAgent`, `ToolRegistry`, the session sink).
//! A free function (`fn next_id() -> String`) cannot be swapped
//! per-host; a closure (`Arc<dyn Fn() -> String>`) works but
//! misses the type — a hand-rolled wrapper needs the same surface
//! the standard impls below already provide.
//!
//! # Default: ULID
//!
//! [`UlidGenerator`] uses the workspace-pinned `ulid` crate:
//! time-ordered, 128-bit, URL-safe, lexicographically sortable.
//! `UlidGenerator::new()` is process-local; the
//! [`UlidGenerator::prefixed`] variant tacks a host tag on the
//! front (`run-01H...`, `sess-01H...`) so logs from different
//! surfaces can be sorted visually.
//!
//! # Test generator
//!
//! [`SequenceGenerator`] mints `"id-0"`, `"id-1"`, `"id-2"`, …
//! Deterministic, allocation-free after construction, exactly
//! what a unit test wants.

use std::sync::Arc;

use parking_lot::Mutex;

/// Source of unique string identifiers.
pub trait IdGen: Send + Sync {
    /// Mint the next identifier. Implementations MUST return a
    /// value that has not been returned by any previous call to
    /// `next_id` on the same generator instance.
    fn next_id(&self) -> String;
}

/// ULID-based generator, the default for the Synthia runtime.
///
/// Each `UlidGenerator` instance owns a `ulid::Generator` behind
/// a mutex, so concurrent calls produce distinct monotonic IDs.
/// The `prefixed` variant prepends a short tag so logs from
/// different surfaces can be told apart at a glance.
pub struct UlidGenerator {
    inner: Mutex<ulid::Generator>,
    prefix: Option<&'static str>,
}

impl std::fmt::Debug for UlidGenerator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut dbg = f.debug_struct("UlidGenerator");
        if let Some(p) = self.prefix {
            dbg.field("prefix", &p);
        }
        dbg.finish_non_exhaustive()
    }
}

impl UlidGenerator {
    /// Create an unprefixed ULID generator.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(ulid::Generator::new()),
            prefix: None,
        }
    }

    /// Create a ULID generator whose IDs are prefixed by `tag`
    /// (e.g. `UlidGenerator::prefixed("run")` → `"run-01H..."`).
    /// The tag must be lowercase ASCII to keep the result URL-safe.
    #[must_use]
    pub fn prefixed(tag: &'static str) -> Self {
        Self {
            inner: Mutex::new(ulid::Generator::new()),
            prefix: Some(tag),
        }
    }
}

impl Default for UlidGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl IdGen for UlidGenerator {
    fn next_id(&self) -> String {
        let ulid = self
            .inner
            .lock()
            .generate()
            .unwrap_or_else(|o| o.commit_overflow_increment());
        match self.prefix {
            Some(tag) => format!("{tag}-{ulid}"),
            None => ulid.to_string(),
        }
    }
}

/// Deterministic sequential generator for tests.
///
/// Returns `"id-0"`, `"id-1"`, … in order. Two generators mint
/// independent sequences; cloning a generator continues the
/// original sequence (because the counter is shared behind the
/// `Arc`). To start a fresh sequence at zero per test, call
/// [`SequenceGenerator::new()`].
#[derive(Clone, Default)]
pub struct SequenceGenerator {
    counter: Arc<Mutex<u64>>,
    tag: &'static str,
}

impl std::fmt::Debug for SequenceGenerator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SequenceGenerator")
            .field("tag", &self.tag)
            .finish_non_exhaustive()
    }
}

impl SequenceGenerator {
    /// Create a new sequence, starting at 0, tagged `id`.
    #[must_use]
    pub fn new() -> Self {
        Self::with_tag("id")
    }

    /// Create a sequence with a custom tag (e.g. `"run"` →
    /// `"run-0"`, `"run-1"`, …).
    #[must_use]
    pub fn with_tag(tag: &'static str) -> Self {
        Self {
            counter: Arc::new(Mutex::new(0)),
            tag,
        }
    }
}

impl IdGen for SequenceGenerator {
    fn next_id(&self) -> String {
        let mut g = self.counter.lock();
        let n = *g;
        *g += 1;
        format!("{}-{n}", self.tag)
    }
}

/// Shared `IdGen` newtype around `Arc<dyn IdGen>`.
///
/// Mirrors [`crate::clock::SharedClock`]: hide the `Arc` / `dyn`
/// noise behind a name that says what it is. Implementations of
/// `IdGen` from this crate (and custom impls) become
/// interchangeable through `SharedIdGen`.
#[derive(Clone)]
pub struct SharedIdGen(Arc<dyn IdGen>);

impl SharedIdGen {
    /// Wrap a concrete `IdGen`.
    #[must_use]
    pub fn new(id_gen: impl IdGen + 'static) -> Self {
        Self(Arc::new(id_gen))
    }

    /// Wrap a pre-built `Arc<dyn IdGen>`.
    #[must_use]
    pub fn from_arc(id_gen: Arc<dyn IdGen>) -> Self {
        Self(id_gen)
    }

    /// Production shorthand: an unprefixed ULID generator.
    #[must_use]
    pub fn ulid() -> Self {
        Self::new(UlidGenerator::new())
    }

    /// Production shorthand: a prefixed ULID generator
    /// (`"run-01H..."`, `"sess-01H..."`, …).
    #[must_use]
    pub fn ulid_prefixed(tag: &'static str) -> Self {
        Self::new(UlidGenerator::prefixed(tag))
    }

    /// Test shorthand: a deterministic sequence generator
    /// (`"id-0"`, `"id-1"`, …).
    #[must_use]
    pub fn sequence() -> Self {
        Self::new(SequenceGenerator::new())
    }
}

impl IdGen for SharedIdGen {
    fn next_id(&self) -> String {
        self.0.next_id()
    }
}

impl std::fmt::Debug for SharedIdGen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedIdGen").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ulid_generator_returns_unique_ids() {
        let g = UlidGenerator::new();
        let a = g.next_id();
        let b = g.next_id();
        assert_ne!(a, b);
    }

    #[test]
    fn ulid_generator_prefixed_uses_tag() {
        let g = UlidGenerator::prefixed("run");
        let id = g.next_id();
        assert!(id.starts_with("run-"), "expected 'run-' prefix, got {id}");
    }

    #[test]
    fn sequence_generator_is_deterministic() {
        let g = SequenceGenerator::with_tag("seq");
        assert_eq!(g.next_id(), "seq-0");
        assert_eq!(g.next_id(), "seq-1");
        assert_eq!(g.next_id(), "seq-2");
    }

    #[test]
    fn sequence_generator_clone_shares_state() {
        let a = SequenceGenerator::with_tag("seq");
        let b = a.clone();
        assert_eq!(a.next_id(), "seq-0");
        assert_eq!(b.next_id(), "seq-1");
    }

    #[test]
    fn shared_idgen_delegates() {
        let g = SharedIdGen::sequence();
        assert_eq!(g.next_id(), "id-0");
    }
}
