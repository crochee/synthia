//! `Job` — one scheduled firing.
//!
//! A [`Job`] is the durable unit: it carries a stable id, a
//! human-readable name, the typed [`JobKind`] that decides
//! when the next fire is due, and a free-form
//! `serde_json::Value` payload that the caller interprets as
//! the spawn request. The scheduler does NOT understand the
//! payload — it stores it, and emits it on `tick` when the
//! job is due. This keeps the scheduler usable by every
//! scheduling consumer (agent spawn, cleanup sweep, email
//! reminder, …) without growing a per-domain enum here.

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Stable opaque identifier for one [`Job`]. ULID-encoded so
/// the lexicographic order of ids also encodes creation
/// order — useful for the store's "next job to fire"
/// queries and for `unfire`-style debugging (newer job ids
/// sort higher).
#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct JobId(String);

impl JobId {
    /// Mint a fresh ULID-backed id.
    #[must_use]
    pub fn new() -> Self {
        Self(ulid::Ulid::generate().to_string())
    }

    /// Construct from a string the caller already minted.
    /// Useful for restore-from-disk and for tests that need a
    /// stable id.
    #[must_use]
    pub fn from_string(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Borrow the underlying string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for JobId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for JobId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One schedule expression. Adding a new variant is a
/// deliberate act — `Scheduler::tick` is the only place
/// that needs to learn the new firing semantics, and every
/// persistence layer round-trips through the same enum.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JobKind {
    /// Cron-style expression in 5-field POSIX format
    /// (`"m h dom mon dow"`). The recurrence is the `cron` feature's
    /// job: with it enabled, `CronTrigger` parses `expr` on every
    /// advance; without it, `advance` re-arms a minute out and no
    /// parser is linked in.
    Cron {
        /// Original cron expression string. Persisted for
        expr: String,
    },
    /// Fixed-interval re-fire. `next_fire_at` is advanced by
    /// `interval` on every successful tick.
    Interval {
        /// Re-fire cadence.
        interval: Duration,
    },
    /// One-shot. The scheduler emits exactly one firing and
    /// then auto-`complete()`s the job — the next tick does
    /// not re-emit it.
    Once,
}

/// Lifecycle status. `Active` is the only state that emits
/// firings. `Paused` keeps the schedule on disk but stops
/// ticking. `Completed` is terminal — the store's
/// `gc_completed()` sweep is the only thing that removes
/// such rows.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    #[default]
    Active,
    Paused,
    Completed,
}

/// Longest re-fire cadence an [`JobKind::Interval`] job may carry, in
/// seconds (10 years).
///
/// A bound is required, not cosmetic: `Job::advance` runs inside the
/// caller's `tick` loop, and an unbounded interval reaches
/// `chrono::Duration::seconds`, which **panics** above
/// `i64::MAX / 1000`. A job with a larger interval would abort the
/// host's tick rather than the call that created it — and because the
/// failed advance is never persisted, it would do so again on every
/// subsequent tick.
///
/// Ten years is far beyond any scheduling horizon a deployment has, and
/// leaves ~13 orders of magnitude of headroom below the representable
/// limit.
pub const MAX_INTERVAL_SECS: u64 = 315_360_000;

/// Error type for [`Job`] operations.
#[derive(Debug, Error)]
pub enum ScheduleError {
    /// `interval` was zero or would not advance the clock.
    #[error("interval must be greater than zero")]
    ZeroInterval,
    /// `interval` exceeds [`MAX_INTERVAL_SECS`], or cannot be
    /// represented as a `chrono::Duration`.
    #[error("interval of {secs}s exceeds the maximum of {max}s")]
    IntervalOutOfRange {
        /// The rejected interval.
        secs: u64,
        /// [`MAX_INTERVAL_SECS`].
        max: u64,
    },
    /// The next fire instant left `chrono`'s representable range.
    #[error("next fire time from {now} + {delta} is out of range")]
    NextFireOutOfRange {
        /// The instant the advance started from.
        now: DateTime<Utc>,
        /// The interval that was applied.
        delta: chrono::Duration,
    },
    /// Stored job carried a `next_fire_at` strictly in the
    /// past on load. The caller chose to fail rather than
    /// silently fire-on-tick.
    #[error("next_fire_at is in the past: {0}")]
    PastNextFire(DateTime<Utc>),
    /// A [`JobKind::Cron`] expression could not be parsed, or has no
    /// reachable occurrence.
    #[error("invalid cron expression {expr:?}: {reason}")]
    InvalidCron {
        /// The rejected expression, as the caller wrote it.
        expr: String,
        /// Why it was rejected, in the parser's own words.
        reason: String,
    },
}

/// One persisted scheduling row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    /// Stable id (ULID by default).
    pub id: JobId,
    /// Operator-facing name. The store's `has_name` check
    /// enforces uniqueness per-store so two jobs cannot
    /// shadow each other in operator dashboards.
    pub name: String,
    /// Free-form description for operator dashboards.
    #[serde(default)]
    pub description: String,
    /// Schedule kind. Drives `next_fire`.
    pub kind: JobKind,
    /// Payload the caller interprets as the spawn request.
    /// The scheduler stores and re-emits it as-is.
    pub payload: serde_json::Value,
    /// Wall-clock time the scheduler should next emit this
    /// job. Persisted so reloads survive a process restart.
    pub next_fire_at: DateTime<Utc>,
    /// Last firing time (UTC). `None` until the first tick
    /// actually fires.
    #[serde(default)]
    pub last_fired_at: Option<DateTime<Utc>>,
    /// Current lifecycle state.
    pub status: JobStatus,
}

impl Job {
    /// Construct a fresh active job with `next_fire_at =
    /// now + initial_offset`. A cron job's first firing is the
    /// caller's to compute — `CronTrigger::first_after(expr, now)`
    /// when the `cron` feature is on — because only the caller
    /// knows which expression the operator asked for. Interval jobs
    /// accept a zero initial offset
    /// so `Interval { interval }` + `next_fire_at = now`
    /// fires "as soon as the scheduler ticks".
    ///
    /// # Errors
    ///
    /// [`ScheduleError::ZeroInterval`] if the interval is
    /// `Duration::ZERO`.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        kind: JobKind,
        payload: serde_json::Value,
        next_fire_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: JobId::new(),
            name: name.into(),
            description: String::new(),
            kind,
            payload,
            next_fire_at,
            last_fired_at: None,
            status: JobStatus::Active,
        }
    }

    /// Builder-style `description` setter.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Builder-style explicit-id setter. Useful for tests
    /// that want a deterministic id and for restore paths
    /// that need to preserve the original id.
    #[must_use]
    pub fn with_id(mut self, id: JobId) -> Self {
        self.id = id;
        self
    }

    /// True when the job's `next_fire_at` has arrived at
    /// `now`. Paused / completed jobs never fire
    /// regardless of `now`.
    #[must_use]
    pub fn is_due(&self, now: DateTime<Utc>) -> bool {
        self.status == JobStatus::Active && self.next_fire_at <= now
    }

    /// Advance `next_fire_at` to the next firing time and
    /// stamp `last_fired_at`. For `Once`, the job is moved
    /// to [`JobStatus::Completed`] (the store's
    /// `gc_completed` sweep is what reaps it).
    ///
    /// # Errors
    ///
    /// [`ScheduleError::ZeroInterval`] if an `Interval`
    /// kind somehow ended up with a zero duration;
    /// [`ScheduleError::IntervalOutOfRange`] when the interval
    /// cannot be represented as a `chrono::Duration` or the
    /// resulting instant would leave the representable range;
    /// [`ScheduleError::InvalidCron`] when a `Cron` job's
    /// expression does not parse (a build without the `cron`
    /// feature re-arms the job a minute out instead).
    ///
    /// This method is **total**: every arithmetic step is checked,
    /// because it runs inside the caller's `tick` loop, where a panic is
    /// not a tool error the model can read but a dead host. A job whose
    /// `interval` exceeds [`MAX_INTERVAL_SECS`] therefore fails here and
    /// is reported, rather than aborting the tick.
    ///
    /// The recurrence itself is computed by the crate-private
    /// `next_fire_of` — the same function the
    /// [`JobTrigger`](crate::trigger::JobTrigger) seam wraps; this
    /// method owns only the state transition and the stamp.
    pub fn advance(&mut self, now: DateTime<Utc>) -> Result<(), ScheduleError> {
        match next_fire_of(&self.kind, now)? {
            None => self.status = JobStatus::Completed,
            Some(next) => self.next_fire_at = next,
        }
        // Stamped only after every fallible step succeeded: a failed
        // advance must not record a firing that never happened, or the
        // store would show a `last fired` time for a job that did not
        // fire (and the tool's `list` renders exactly that field).
        self.last_fired_at = Some(now);
        Ok(())
    }
}

/// The recurrence behind [`Job::advance`]: where `kind` fires next
/// after `now`, or `None` when it is terminal.
///
/// Crate-private because it is the single implementation of the
/// recurrence — [`JobTrigger`](crate::trigger::JobTrigger) wraps this
/// same function, so the two can never drift apart.
///
/// # Errors
///
/// The [`ScheduleError`]s [`Job::advance`] documents.
pub(crate) fn next_fire_of(
    kind: &JobKind,
    now: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>, ScheduleError> {
    match kind {
        // With the parser compiled in, the recurrence is the
        // expression's own: a malformed expression is a typed error
        // here, which `advance` surfaces instead of re-arming the job
        // at a silently wrong next fire.
        #[cfg(feature = "cron")]
        JobKind::Cron { expr } => {
            crate::trigger::CronTrigger::first_after(expr, now).map(Some)
        }
        // Placeholder; with the `cron` feature enabled this is the real
        // recurrence. A default build re-arms a cron job one minute out
        // and never grows a parser dependency.
        #[cfg(not(feature = "cron"))]
        JobKind::Cron { .. } => {
            Ok(Some(add(now, chrono::Duration::minutes(1))?))
        }
        JobKind::Interval { interval } => {
            if interval.is_zero() {
                return Err(ScheduleError::ZeroInterval);
            }
            if interval.as_secs() > MAX_INTERVAL_SECS {
                return Err(ScheduleError::IntervalOutOfRange {
                    secs: interval.as_secs(),
                    max: MAX_INTERVAL_SECS,
                });
            }
            // `chrono::Duration::seconds` panics above
            // `i64::MAX / 1000`; `try_seconds` is the total form.
            // The bound above already excludes that range, so this
            // is belt-and-braces against a future relaxation of the
            // constant.
            let delta = chrono::Duration::try_seconds(
                i64::try_from(interval.as_secs()).map_err(|_| {
                    ScheduleError::IntervalOutOfRange {
                        secs: interval.as_secs(),
                        max: MAX_INTERVAL_SECS,
                    }
                })?,
            )
            .ok_or(ScheduleError::IntervalOutOfRange {
                secs: interval.as_secs(),
                max: MAX_INTERVAL_SECS,
            })?;
            Ok(Some(add(now, delta)?))
        }
        JobKind::Once => Ok(None),
    }
}

/// `now + delta`, or a typed error when the instant would leave
/// `chrono`'s representable range (a `DateTime` addition panics
/// otherwise).
fn add(
    now: DateTime<Utc>,
    delta: chrono::Duration,
) -> Result<DateTime<Utc>, ScheduleError> {
    now.checked_add_signed(delta)
        .ok_or(ScheduleError::NextFireOutOfRange { now, delta })
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn at(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).single().unwrap()
    }

    #[test]
    fn due_only_when_active_and_past_next_fire() {
        let mut job =
            Job::new("t", JobKind::Once, serde_json::json!({}), at(100));
        assert!(job.is_due(at(100)));
        assert!(job.is_due(at(101)));
        job.status = JobStatus::Paused;
        assert!(!job.is_due(at(101)));
        job.status = JobStatus::Active;
        assert!(!job.is_due(at(99)));
    }

    #[test]
    fn once_advance_marks_completed() {
        let mut mut_job =
            Job::new("once", JobKind::Once, serde_json::json!({}), at(100));
        mut_job.advance(at(100)).unwrap();
        assert_eq!(mut_job.status, JobStatus::Completed);
        assert_eq!(mut_job.last_fired_at, Some(at(100)));
    }

    #[test]
    fn interval_advance_rolls_next_fire_by_interval() {
        let mut mut_job = Job::new(
            "every-5s",
            JobKind::Interval {
                interval: Duration::from_secs(5),
            },
            serde_json::json!({}),
            at(100),
        );
        mut_job.advance(at(100)).unwrap();
        assert_eq!(mut_job.next_fire_at, at(105));
        assert_eq!(mut_job.status, JobStatus::Active);
    }

    #[test]
    fn zero_interval_rejected() {
        let mut mut_job = Job::new(
            "bad",
            JobKind::Interval {
                interval: Duration::ZERO,
            },
            serde_json::json!({}),
            at(0),
        );
        let err = mut_job.advance(at(0)).unwrap_err();
        assert!(matches!(err, ScheduleError::ZeroInterval));
    }

    /// An interval in `chrono`'s panicking range must be reported, not
    /// panicked on: `advance` runs inside the caller's `tick` loop, so a
    /// panic there aborts the host rather than failing a tool call.
    #[test]
    fn out_of_range_interval_is_an_error_not_a_panic() {
        for interval in [
            Duration::from_secs(10_000_000_000_000_000), // 1e16
            Duration::from_secs(u64::MAX),               // wraps in `as i64`
            Duration::from_secs(MAX_INTERVAL_SECS + 1),
        ] {
            let mut job = Job::new(
                "absurd",
                JobKind::Interval { interval },
                serde_json::json!({}),
                at(0),
            );
            let err = job.advance(at(0)).unwrap_err();
            assert!(
                matches!(err, ScheduleError::IntervalOutOfRange { .. }),
                "{err:?}"
            );
        }
    }

    /// The documented maximum is itself schedulable — the bound rejects
    /// only what the crate cannot represent.
    #[test]
    fn the_maximum_interval_advances() {
        let mut job = Job::new(
            "ten-years",
            JobKind::Interval {
                interval: Duration::from_secs(MAX_INTERVAL_SECS),
            },
            serde_json::json!({}),
            at(0),
        );
        job.advance(at(0)).expect("the bound must itself work");
        assert_eq!(job.next_fire_at, at(MAX_INTERVAL_SECS as i64));
    }

    /// A failed advance must not record a firing: `last_fired_at` is
    /// rendered by the tool's `list`, so stamping it before the fallible
    /// steps would show a fire time for a job that never fired.
    #[test]
    fn a_failed_advance_does_not_stamp_last_fired() {
        let mut job = Job::new(
            "absurd",
            JobKind::Interval {
                interval: Duration::from_secs(u64::MAX),
            },
            serde_json::json!({}),
            at(0),
        );
        job.advance(at(500)).unwrap_err();
        assert_eq!(job.last_fired_at, None);
        assert_eq!(job.next_fire_at, at(0), "and it did not advance");
    }

    /// A `Cron` advance is also checked: the one-minute placeholder is
    /// safe, but the addition must stay total for any `now`. With the
    /// `cron` feature on the placeholder is gone — see
    /// `cron_advance_follows_the_expression`.
    #[cfg(not(feature = "cron"))]
    #[test]
    fn cron_advance_is_total() {
        let mut job = Job::new(
            "cron",
            JobKind::Cron {
                expr: "0 9 * * *".into(),
            },
            serde_json::json!({}),
            at(0),
        );
        job.advance(at(0)).expect("cron advance");
        assert_eq!(job.next_fire_at, at(60));
    }

    /// With the parser compiled in, `advance` re-arms the job at the
    /// expression's real next occurrence instead of the placeholder.
    #[cfg(feature = "cron")]
    #[test]
    fn cron_advance_follows_the_expression() {
        let mut job = Job::new(
            "cron",
            JobKind::Cron {
                expr: "0 9 * * *".into(),
            },
            serde_json::json!({}),
            at(0),
        );
        job.advance(at(0)).expect("cron advance");
        assert_eq!(job.next_fire_at, at(9 * 3600));
    }

    /// An expression the parser rejects fails the advance rather than
    /// re-arming: the job keeps its fire time and does not stamp a
    /// firing that never happened.
    #[cfg(feature = "cron")]
    #[test]
    fn cron_advance_reports_a_bad_expression() {
        let mut job = Job::new(
            "cron",
            JobKind::Cron {
                expr: "not cron".into(),
            },
            serde_json::json!({}),
            at(100),
        );
        let err = job.advance(at(100)).unwrap_err();
        assert!(matches!(err, ScheduleError::InvalidCron { .. }), "{err:?}");
        assert_eq!(job.next_fire_at, at(100), "and it did not advance");
        assert_eq!(job.last_fired_at, None);
    }
}
