//! Test-only helpers shared across the crate's `#[cfg(test)]`
//! modules.
//!
//! Anything that exercises skill discovery touches the
//! process-global `$HOME` (user-level skills live under
//! `$HOME/{.claude,.agents}/skills`). Tests that read discovery
//! output MUST scope `$HOME` through [`with_scoped_home`] so
//! they see a deterministic environment instead of the
//! developer's installed skill set, and so concurrent tests
//! serialise their env mutation through [`HOME_LOCK`].

use std::sync::Mutex;

/// Serialize tests that mutate `$HOME` — the env var is
/// process-global and parallel tests would step on each
/// other.
pub(crate) static HOME_LOCK: Mutex<()> = Mutex::new(());

/// RAII helper: pin `$HOME` to a tempdir for the lifetime
/// of the guard so tests don't pick up the developer's
/// installed `.claude/skills/` set.
pub(crate) struct ScopedHome {
    previous: Option<std::ffi::OsString>,
    dir: tempfile::TempDir,
}

impl ScopedHome {
    /// The caller MUST hold [`HOME_LOCK`] for the lifetime
    /// of the returned guard (see [`with_scoped_home`]).
    /// Splitting the lock acquire from `new()` keeps the
    /// critical section minimal and avoids re-entrant
    /// `std::sync::Mutex` deadlocks if `new()` is called
    /// from a context that already holds the lock.
    pub(crate) fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let previous = std::env::var_os("HOME");
        // SAFETY: test-only env mutation, serialized through
        // `HOME_LOCK`.
        unsafe {
            std::env::set_var("HOME", dir.path());
        }
        Self { previous, dir }
    }

    /// The pinned `$HOME` path.
    pub(crate) fn path(&self) -> &std::path::Path {
        self.dir.path()
    }
}

impl Drop for ScopedHome {
    fn drop(&mut self) {
        // `set_var` / `remove_var` are `unsafe` in Rust 2024
        // edition. Test-only env mutation is safe
        // (single-threaded `Drop`, no concurrent reads), so the
        // `unsafe` block is here purely to satisfy the
        // compiler.
        unsafe {
            match self.previous.take() {
                Some(v) => std::env::set_var("HOME", v),
                None => {
                    std::env::remove_var("HOME");
                }
            }
        }
    }
}

/// Run a closure with `$HOME` pinned to a tempdir.
/// Returns whatever the closure returns. RAII restores
/// `$HOME` on scope exit (panic or success).
///
/// The closure runs synchronously while the lock is held —
/// drive async work to completion inside it (e.g.
/// `futures::executor::block_on`) rather than returning a
/// future that escapes the scoped section.
pub(crate) fn with_scoped_home<F, R>(f: F) -> R
where
    F: FnOnce(&std::path::Path) -> R,
{
    // `unwrap_or_else(|e| e.into_inner())` ignores a
    // poisoned mutex: if a sibling test panicked while
    // holding the lock, we still want subsequent tests
    // to run (the env restoration is idempotent).
    let _guard = HOME_LOCK
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let scoped = ScopedHome::new();
    f(scoped.path())
}
