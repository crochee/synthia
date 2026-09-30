//! `JobTrigger` — the recurrence seam.
//!
//! A trigger answers exactly one question: given `now`, when does this
//! schedule fire next? `None` means the schedule has no future firing
//! — terminal — which `Job::advance` maps onto
//! [`JobStatus::Completed`](crate::job::JobStatus::Completed).
//!
//! The impls here are thin wrappers over the same recurrence
//! [`Job::advance`](crate::job::Job::advance) runs, so a scheduler
//! re-arming a job and the job's own advance can never disagree about
//! what "every 5s" means. `Cron` (feature `cron`) is the one trigger
//! that owns its recurrence instead of wrapping it: `advance`'s Cron
//! arm calls `CronTrigger::first_after`, so the two sides still share
//! one parser.

use std::time::Duration;

use chrono::{DateTime, Utc};

#[cfg(feature = "cron")]
use crate::job::ScheduleError;
use crate::job::{JobKind, next_fire_of};

/// Recurrence of one schedule: `now` in, next firing out.
///
/// `Send + Sync` because a trigger lives inside a [`Job`](crate::Job)
/// owned by the scheduler, which callers drive from their own
/// executor.
pub trait JobTrigger: Send + Sync {
    /// The next firing time after `now`, or `None` when the schedule
    /// is terminal (it will not fire again).
    fn next_fire(&self, now: DateTime<Utc>) -> Option<DateTime<Utc>>;

    /// Human-readable cadence, in the same wording the `schedule`
    /// tool renders (`once`, `every 5s`).
    fn describe(&self) -> String;
}

/// A schedule that fires exactly once and then stops.
///
/// The trigger itself is terminal from the start: moving a `Once` job
/// to `Completed` is `Job::advance`'s job, so a caller that asks this
/// trigger for a next firing always gets `None`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OnceTrigger;

impl JobTrigger for OnceTrigger {
    fn next_fire(&self, _now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        None
    }

    fn describe(&self) -> String {
        "once".to_string()
    }
}

/// A schedule that re-fires every `interval`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntervalTrigger {
    /// Re-fire cadence.
    pub interval: Duration,
}

impl JobTrigger for IntervalTrigger {
    /// `now + interval`.
    ///
    /// A cadence this crate cannot represent (`Duration::ZERO`, or
    /// beyond [`MAX_INTERVAL_SECS`](crate::job::MAX_INTERVAL_SECS))
    /// has no next firing and reports `None`. Callers that need the
    /// reason ask `Job::advance`, which runs the same check and
    /// surfaces it as a [`crate::ScheduleError`].
    fn next_fire(&self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        next_fire_of(
            &JobKind::Interval {
                interval: self.interval,
            },
            now,
        )
        .ok()
        .flatten()
    }

    fn describe(&self) -> String {
        format!("every {}s", self.interval.as_secs())
    }
}

/// A schedule that fires at the instants a cron expression names.
///
/// The expression is 5-field POSIX (`m h dom mon dow`). Three things
/// differ from a POSIX `cron` implementation, and each is settled here
/// rather than left to the parser underneath:
///
/// - **No seconds field.** The parser reads one; this type supplies `0`,
///   so occurrences land on a minute boundary and `*/5 * * * *` means
///   every five minutes rather than every five seconds.
/// - **Sunday is 0 (or 7).** The parser numbers Sunday 1; the
///   day-of-week field is translated before it is handed over, so the
///   POSIX numbering is what an operator writes.
/// - **A restricted day-of-week narrows a restricted day-of-month.**
///   POSIX joins the two fields: it fires when *either* matches. The
///   parser fires only where both match. `0 0 1 * MON` therefore fires
///   on the 1st only when the 1st is a Monday, not on the 1st and every
///   Monday.
///
/// 6- and 7-field expressions are refused rather than guessed at.
#[cfg(feature = "cron")]
#[derive(Clone, Debug)]
pub struct CronTrigger {
    schedule: cron::Schedule,
    expr: String,
}

#[cfg(feature = "cron")]
impl CronTrigger {
    /// Parse a 5-field POSIX expression (`m h dom mon dow`).
    ///
    /// See the type documentation for the three places this differs from
    /// POSIX — the one that bites an operator migrating an expression is
    /// the day-of-month/day-of-week pair: a restricted day-of-week
    /// narrows a restricted day-of-month instead of joining it.
    ///
    /// # Errors
    ///
    /// [`ScheduleError::InvalidCron`] when the expression does not have
    /// exactly five fields, or a field is not one the parser accepts.
    pub fn parse(expr: &str) -> Result<Self, ScheduleError> {
        Ok(Self {
            schedule: parse_schedule(expr)?,
            expr: expr.to_string(),
        })
    }

    /// The first firing strictly after `from` — a job is never re-armed
    /// onto the instant it just fired.
    ///
    /// # Errors
    ///
    /// [`ScheduleError::InvalidCron`] when the expression does not
    /// parse, or names no reachable instant (a day-of-month the month
    /// never has, say).
    pub fn first_after(
        expr: &str,
        from: DateTime<Utc>,
    ) -> Result<DateTime<Utc>, ScheduleError> {
        parse_schedule(expr)?.after(&from).next().ok_or_else(|| {
            ScheduleError::InvalidCron {
                expr: expr.to_string(),
                reason: "expression has no reachable firing".to_string(),
            }
        })
    }
}

#[cfg(feature = "cron")]
impl JobTrigger for CronTrigger {
    fn next_fire(&self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        self.schedule.after(&now).next()
    }

    fn describe(&self) -> String {
        format!("cron `{}`", self.expr)
    }
}

/// The parser's schedule for a 5-field POSIX expression.
///
/// The parser reads `sec min hour dom mon dow [year]`; POSIX cron has no
/// seconds field. Prepending `0` is what makes `*/5 * * * *` mean "every
/// five minutes" rather than "every five seconds", and the day-of-week
/// field is rewritten to the parser's numbering (see
/// [`posix_day_of_week`]). What this does *not* rewrite is the parser's
/// treatment of `dom` and `dow` together — it intersects them where
/// POSIX joins them, which [`CronTrigger`] documents.
#[cfg(feature = "cron")]
fn parse_schedule(expr: &str) -> Result<cron::Schedule, ScheduleError> {
    let fields: Vec<&str> = expr.split_whitespace().collect();
    let [minute, hour, day_of_month, month, day_of_week] = fields[..] else {
        return Err(invalid_cron(
            expr,
            format!(
                "expected 5 fields (m h dom mon dow), got {}",
                fields.len()
            ),
        ));
    };
    let day_of_week = posix_day_of_week(day_of_week)
        .map_err(|reason| invalid_cron(expr, reason))?;
    format!("0 {minute} {hour} {day_of_month} {month} {day_of_week}")
        .parse::<cron::Schedule>()
        .map_err(|err| invalid_cron(expr, err.to_string()))
}

#[cfg(feature = "cron")]
fn invalid_cron(expr: &str, reason: String) -> ScheduleError {
    ScheduleError::InvalidCron {
        expr: expr.to_string(),
        reason,
    }
}

/// Rewrite a POSIX day-of-week field into the parser's numbering, where
/// Sunday is 1 rather than 0.
///
/// Day names mean the same in both numberings and pass through
/// untouched, step or no step, as do `*` and `?`. A single ordinal is
/// shifted (`5`, Friday in POSIX, becomes the parser's 6); a numeric
/// range is written out day by day (`1-5` becomes `2,3,4,5`) because the
/// shift moves the end of a range that includes Sunday (`5-7` is
/// Fri–Sun, `6,7,1` afterwards).
///
/// A *step* on numeric ordinals is refused rather than guessed at: the
/// step counts POSIX days, so shifting the endpoints would silently move
/// the cadence by a day. Names carry the step correctly
/// (`MON-FRI/2`), so the reason says so and the caller can recover.
///
/// # Errors
///
/// The reason to report, phrased for the operator.
#[cfg(feature = "cron")]
fn posix_day_of_week(field: &str) -> Result<String, String> {
    field
        .split(',')
        .map(posix_day_element)
        .collect::<Result<Vec<_>, _>>()
        .map(|days| days.join(","))
}

/// One comma-free element of a POSIX day-of-week field.
///
/// # Errors
///
/// The reason to report, phrased for the operator: a stepped numeric
/// element and a non-POSIX element are different mistakes, so they do
/// not share a message.
#[cfg(feature = "cron")]
fn posix_day_element(element: &str) -> Result<String, String> {
    let (base, stepped) = match element.split_once('/') {
        Some((base, _)) => (base, true),
        None => (element, false),
    };
    if base == "*"
        || base == "?"
        || base.chars().any(|c| c.is_ascii_alphabetic())
    {
        return Ok(element.to_string());
    }
    if stepped {
        return Err(format!(
            "day of week {element:?} steps over numeric ordinals, whose \
             POSIX-day shift would move the cadence by a day; write the \
             days by name instead (e.g. MON-FRI/2)"
        ));
    }
    match base.split_once('-') {
        Some((first, last)) => match (posix_day(first), posix_day(last)) {
            (Some(first), Some(last)) => Ok((first..=last)
                .map(shift_day)
                .map(|day| day.to_string())
                .collect::<Vec<_>>()
                .join(",")),
            _ => Err(not_a_posix_day(element)),
        },
        None => posix_day(base)
            .map(shift_day)
            .map(|day| day.to_string())
            .ok_or_else(|| not_a_posix_day(element)),
    }
}

/// Why a day-of-week element that is not a stepped numeric one was
/// refused: it names no POSIX day.
#[cfg(feature = "cron")]
fn not_a_posix_day(element: &str) -> String {
    format!(
        "day of week {element:?} is not a POSIX day (0-7, or a name \
         like MON)"
    )
}

/// The POSIX ordinal a day-of-week field names, or `None` when it is not
/// one (out of `0`–`7`, or not a number at all). Also used for a range's
/// two endpoints.
#[cfg(feature = "cron")]
fn posix_day(field: &str) -> Option<u32> {
    field.trim().parse::<u32>().ok().filter(|day| *day <= 7)
}

/// The parser's ordinal for a POSIX day of week: Sunday is 0 — or 7 —
/// in POSIX and 1 here.
#[cfg(feature = "cron")]
fn shift_day(day: u32) -> u32 {
    day % 7 + 1
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use chrono::{TimeZone, Utc};

    use super::*;

    fn at(secs: i64) -> chrono::DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).single().unwrap()
    }

    #[test]
    fn interval_trigger_rolls_forward() {
        let t = IntervalTrigger {
            interval: Duration::from_secs(5),
        };
        assert_eq!(t.next_fire(at(100)), Some(at(105)));
        assert_eq!(t.describe(), "every 5s");
    }

    #[test]
    fn once_trigger_is_terminal() {
        assert_eq!(OnceTrigger.next_fire(at(0)), None);
        assert_eq!(OnceTrigger.describe(), "once");
    }

    #[test]
    fn interval_trigger_rejects_unrepresentable_intervals() {
        for interval in [
            Duration::ZERO,
            Duration::from_secs(crate::job::MAX_INTERVAL_SECS + 1),
            Duration::from_secs(u64::MAX),
        ] {
            let t = IntervalTrigger { interval };
            assert_eq!(t.next_fire(at(0)), None, "{interval:?}");
        }
    }
}

#[cfg(test)]
#[cfg(feature = "cron")]
mod cron_tests {
    use chrono::{TimeZone, Utc};

    use super::*;
    fn at(secs: i64) -> chrono::DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).single().unwrap()
    }

    #[test]
    fn five_field_every_5_min() {
        let t = CronTrigger::parse("*/5 * * * *").unwrap();
        // at(0)=1970-01-01T00:00:00Z → 下一班 00:05
        assert_eq!(t.next_fire(at(0)), Some(at(300)));
    }

    #[test]
    fn first_after_strictly_increases() {
        let t = CronTrigger::parse("0 9 * * *").unwrap();
        assert_eq!(t.next_fire(at(0)), Some(at(9 * 3600)));
        assert_eq!(t.next_fire(at(9 * 3600)), Some(at(9 * 3600 + 86400)));
    }

    #[test]
    fn six_field_expression_is_rejected() {
        assert!(CronTrigger::parse("0 */5 * * * *").is_err());
    }

    #[test]
    fn garbage_is_invalid_cron() {
        let err = CronTrigger::parse("not cron").unwrap_err();
        assert!(matches!(err, crate::ScheduleError::InvalidCron { .. }));
    }

    /// The `dow` field is POSIX: 1 is Monday, 0 and 7 are Sunday. 1970-01-01
    /// was a Thursday, so `0 9 * * 5` (Friday) lands on the 2nd, and a
    /// Mon–Fri range fires Thursday *and* Friday rather than skipping to
    /// Sunday — the shift a naive hand-off to the parser would produce.
    #[test]
    fn day_of_week_is_posix() {
        let friday = CronTrigger::first_after("0 9 * * 5", at(0)).unwrap();
        assert_eq!(friday, at(86_400 + 9 * 3600));
        for sunday in ["0 9 * * 0", "0 9 * * 7"] {
            assert_eq!(
                CronTrigger::first_after(sunday, at(0)).unwrap(),
                at(3 * 86_400 + 9 * 3600),
                "{sunday}"
            );
        }
        let weekdays = CronTrigger::parse("0 9 * * 1-5").unwrap();
        assert_eq!(weekdays.next_fire(at(0)), Some(at(9 * 3600)));
        assert_eq!(
            weekdays.next_fire(at(9 * 3600)),
            Some(at(86_400 + 9 * 3600))
        );
    }

    /// A stepped numeric day-of-week field counts POSIX days, which the
    /// shift does not preserve — it is refused, not silently moved by a
    /// day — and it is refused with its *own* reason: an operator who
    /// wrote `1-5/2` needs the name-form hint, not "not a POSIX day".
    /// The name spelling carries the same cadence correctly.
    #[test]
    fn stepped_numeric_day_of_week_is_refused() {
        assert!(CronTrigger::parse("0 9 * * MON-FRI/2").is_ok());
        let err = CronTrigger::parse("0 9 * * 1-5/2").unwrap_err();
        let crate::ScheduleError::InvalidCron { reason, .. } = err else {
            panic!("expected InvalidCron, got {err:?}");
        };
        assert!(reason.contains("name"), "{reason}");
        assert!(!reason.contains("is not a POSIX day"), "{reason}");

        let err = CronTrigger::parse("0 9 * * 9").unwrap_err();
        let crate::ScheduleError::InvalidCron { reason, .. } = err else {
            panic!("expected InvalidCron, got {err:?}");
        };
        assert!(reason.contains("is not a POSIX day"), "{reason}");

        // An unknown *name* never reaches the translation — the parser
        // owns that message, and it is still an `InvalidCron`.
        let err = CronTrigger::parse("0 9 * * BEAR").unwrap_err();
        assert!(
            matches!(err, crate::ScheduleError::InvalidCron { .. }),
            "{err:?}"
        );
    }

    /// The parser intersects `dom` and `dow` where POSIX joins them: with
    /// both restricted, a firing needs both to match. 1970-01-01 was a
    /// Thursday, so `0 9 1 * 5` waits for a 1st that is also a Friday —
    /// 1970-05-01, 120 days out. Under POSIX's disjunction the first
    /// firing would instead be the very next Friday, 1970-01-02.
    #[test]
    fn restricted_day_of_month_intersects_day_of_week() {
        let both = CronTrigger::first_after("0 9 1 * 5", at(0)).unwrap();
        assert_eq!(both, at(120 * 86_400 + 9 * 3600));
        let friday = CronTrigger::first_after("0 9 * * 5", at(0)).unwrap();
        assert_eq!(friday, at(86_400 + 9 * 3600), "the union would fire here");
    }

    /// An expression that parses but can never fire is an error rather
    /// than an empty schedule: February has no 31st.
    #[test]
    fn unreachable_expression_is_invalid_cron() {
        assert!(CronTrigger::parse("0 0 31 2 *").is_ok());
        let err = CronTrigger::first_after("0 0 31 2 *", at(0)).unwrap_err();
        assert!(
            matches!(err, crate::ScheduleError::InvalidCron { .. }),
            "{err:?}"
        );
    }
}
