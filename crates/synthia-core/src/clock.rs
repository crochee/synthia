//! Wall-clock abstraction — `chrono::DateTime<Utc>` under a trait.
//!
//! # Why a trait, not a function pointer
//!
//! Production code passes time through a trait so that tests can
//! inject a fixed value. The `synthia-harness` ReAct loop and any
//! custom agent do this manually with
//! `Arc<dyn Fn() -> chrono::DateTime<chrono::Utc> + Send + Sync>`
//! before this module existed. Centralising the pattern here
//! gives the same testability with a real type —
//! [`SharedClock::fixed_at`] — and removes the repeated
//! `Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>` prose.
//!
//! # Why chrono, not `std::time::Instant`
//!
//! Wall-clock timestamps carry timezone information and serialize
//! to RFC3339; monotonic durations do not. `chrono::DateTime<Utc>`
//! is the canonical wall-clock type across the workspace.
//!
//! # When to use this vs `std::time::Instant`
//!
//! Use [`Clock`] for **wall-clock** timestamps: session starts,
//! event timestamps, log entries — anything a user or operator
//! reads. Use [`std::time::Instant`] for **monotonic durations**:
//! timeouts, elapsed-time measurements, deadline math. They are
//! complementary; the workspace uses both.
//!
//! # Runtime neutrality
//!
//! [`Clock`] has no `async` methods and no tokio / runtime
//! dependency. It is the consumer's choice whether to read it
//! from a runtime-aware source (e.g. behind a tokio task) or a
//! plain std thread.

use std::sync::Arc;

use chrono::{DateTime, TimeZone, Utc};

/// Wall-clock source.
///
/// Implementations are open. Two are provided by the crate:
///
/// - [`SystemClock`] — production, delegates to `chrono::Utc::now()`.
/// - [`FixedClock`] — tests, pinned to a chosen instant.
///
/// Custom implementations are welcome but should return a
/// `DateTime<Utc>` close to real time so that downstream `Debug`
/// / `Display` output (timestamps in events, session logs,
/// telemetry) stays meaningful.
pub trait Clock: Send + Sync {
    /// The current wall-clock instant.
    fn now(&self) -> DateTime<Utc>;
}

/// Production clock: delegates to [`chrono::Utc::now`].
///
/// Cheap to clone (single `Arc`), thread-safe, allocation-free.
/// The default clock for every Synthia runtime.
#[derive(Debug, Default, Clone)]
pub struct SystemClock;

impl Clock for SystemClock {
    #[inline]
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Test clock: always returns the instant supplied at
/// construction. `now()` never advances.
///
/// The instance owns its instant, so two clocks can hold
/// different fixed times. `Clock::fixed_at(t)` is a shorthand
/// for `FixedClock::new(t)`; both forms exist because call
/// sites often already have a `DateTime<Utc>` in scope.
#[derive(Debug, Clone)]
pub struct FixedClock {
    now: DateTime<Utc>,
}

impl FixedClock {
    /// Pin the clock to `now`. Subsequent `now()` calls return
    /// the same value.
    #[must_use]
    pub fn new(now: DateTime<Utc>) -> Self {
        Self { now }
    }

    /// Convenience constructor for an RFC3339 string. Returns
    /// the unix epoch on parse failure so test setup never
    /// panics; use [`FixedClock::new`] when an invalid date
    /// would be a bug.
    #[must_use]
    pub fn from_rfc3339(s: &str) -> Self {
        let now = DateTime::parse_from_rfc3339(s)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| {
                Utc.timestamp_opt(0, 0).single().unwrap_or_else(Utc::now)
            });
        Self { now }
    }

    /// The instant this clock was pinned to.
    #[must_use]
    pub fn instant(&self) -> DateTime<Utc> {
        self.now
    }
}

impl Clock for FixedClock {
    #[inline]
    fn now(&self) -> DateTime<Utc> {
        self.now
    }
}

/// Shared-clock newtype around `Arc<dyn Clock + Send + Sync>`.
///
/// Most call sites only need to call `.now()`; the wrapper hides
/// the `Arc` / `dyn` so signatures read `SharedClock` instead of
/// `Arc<dyn Clock + Send + Sync>`. The `Clock` impl delegates to
/// the inner trait object, so a `SharedClock` is interchangeable
/// with any other `Clock` source.
#[derive(Clone)]
pub struct SharedClock(Arc<dyn Clock>);

impl SharedClock {
    /// Wrap an existing [`Clock`].
    #[must_use]
    pub fn new(clock: impl Clock + 'static) -> Self {
        Self(Arc::new(clock))
    }

    /// Wrap a pre-built `Arc<dyn Clock>` (e.g. one shared
    /// across many owners).
    #[must_use]
    pub fn from_arc(clock: Arc<dyn Clock>) -> Self {
        Self(clock)
    }

    /// Production clock shorthand: a shared [`SystemClock`].
    #[must_use]
    pub fn system() -> Self {
        Self::new(SystemClock)
    }

    /// Test clock shorthand: pin `now` to the given instant.
    #[must_use]
    pub fn fixed_at(t: DateTime<Utc>) -> Self {
        Self::new(FixedClock::new(t))
    }

    /// Test clock shorthand from an RFC3339 string.
    #[must_use]
    pub fn fixed_from_rfc3339(s: &str) -> Self {
        Self::new(FixedClock::from_rfc3339(s))
    }
}

impl Clock for SharedClock {
    #[inline]
    fn now(&self) -> DateTime<Utc> {
        self.0.now()
    }
}

impl std::fmt::Debug for SharedClock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedClock").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_clock_returns_close_to_now() {
        let clock = SystemClock;
        let before = Utc::now();
        let now = clock.now();
        let after = Utc::now();
        // Allow a 5 ms slop on either side.
        assert!(now >= before - chrono::Duration::milliseconds(5));
        assert!(now <= after + chrono::Duration::milliseconds(5));
    }

    #[test]
    fn fixed_clock_is_pinned() {
        let pinned = Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap();
        let clock = FixedClock::new(pinned);
        assert_eq!(clock.now(), pinned);
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert_eq!(clock.now(), pinned, "FixedClock must not advance");
    }

    #[test]
    fn fixed_clock_from_rfc3339_parses() {
        let clock = FixedClock::from_rfc3339("2026-01-01T12:00:00Z");
        assert_eq!(
            clock.now(),
            Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap()
        );
    }

    #[test]
    fn shared_clock_delegates() {
        let pinned = Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap();
        let clock = SharedClock::fixed_at(pinned);
        assert_eq!(clock.now(), pinned);
    }

    #[test]
    fn shared_clock_is_clone_shares_state() {
        let pinned = Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap();
        let clock = SharedClock::fixed_at(pinned);
        let other = clock.clone();
        assert_eq!(clock.now(), other.now());
    }
}
