//! SpecTrigger implementation for cron-based scheduling

use std::sync::atomic::{AtomicI64, Ordering};

use async_trait::async_trait;
use chrono::{DateTime, Datelike, TimeZone, Timelike};
use chrono_tz::Tz;

use super::calendar::add_months;
use crate::trigger::Trigger;

pub(super) const STAR_BIT: u64 = 1 << 63;

#[derive(Debug, Clone)]
pub struct CronSpec {
    pub second: u64,
    pub minute: u64,
    pub hour: u64,
    pub dom: u64,
    pub month: u64,
    pub dow: u64,
    pub location: Option<Tz>,
    pub spec: String,
}

#[derive(Debug)]
pub struct SpecTrigger {
    second: u64,
    minute: u64,
    hour: u64,
    dom: u64,
    month: u64,
    dow: u64,
    location: Option<Tz>,
    spec: String,
    cache_prev: AtomicI64,
    cache_next: AtomicI64,
}

impl SpecTrigger {
    pub fn new(spec: CronSpec) -> Self {
        Self {
            second: spec.second,
            minute: spec.minute,
            hour: spec.hour,
            dom: spec.dom,
            month: spec.month,
            dow: spec.dow,
            location: spec.location,
            spec: spec.spec,
            cache_prev: AtomicI64::new(i64::MIN),
            cache_next: AtomicI64::new(i64::MIN),
        }
    }

    fn next_time(
        &self,
        t: DateTime<chrono::Utc>,
    ) -> Option<DateTime<chrono::Utc>> {
        let mut t: DateTime<Tz> = self
            .location
            .map(|loc| t.with_timezone(&loc))
            .unwrap_or_else(|| t.with_timezone(&chrono_tz::UTC));

        t += chrono::Duration::seconds(1);
        t = t.with_nanosecond(0).unwrap_or(t);

        let mut added = false;
        let year_limit = t.year() + 5;

        loop {
            if t.year() > year_limit {
                return None;
            }

            let month_bit = 1u64 << (t.month() as u64 - 1);
            if month_bit & self.month == 0 {
                if !added {
                    added = true;
                    t = t
                        .with_day(1)?
                        .with_hour(0)?
                        .with_minute(0)?
                        .with_second(0)?;
                }
                t = add_months(t, 1);
                if t.month() == 1 {
                    continue;
                }
            } else {
                break;
            }
        }

        loop {
            if t.year() > year_limit {
                return None;
            }

            if !self.day_matches(&t) {
                if !added {
                    added = true;
                    t = t.with_hour(0)?.with_minute(0)?.with_second(0)?;
                }
                t += chrono::Duration::days(1);

                if t.hour() != 0 {
                    t = if t.hour() > 12 {
                        t + chrono::Duration::hours(24 - t.hour() as i64)
                    } else {
                        t - chrono::Duration::hours(t.hour() as i64)
                    };
                }

                if t.day() == 1 {
                    let month_bit = 1u64 << (t.month() as u64 - 1);
                    if month_bit & self.month == 0 {
                        continue;
                    }
                }
            } else {
                break;
            }
        }

        'hour_loop: loop {
            if t.year() > year_limit {
                return None;
            }

            let hour_bit = 1u64 << t.hour() as u64;
            if hour_bit & self.hour == 0 {
                if !added {
                    added = true;
                    t = t.with_minute(0)?.with_second(0)?;
                }
                t += chrono::Duration::hours(1);

                if t.hour() == 0 && !self.day_matches(&t) {
                    continue;
                }
                continue;
            }

            loop {
                if t.year() > year_limit {
                    return None;
                }

                let minute_bit = 1u64 << t.minute() as u64;
                if minute_bit & self.minute == 0 {
                    if !added {
                        added = true;
                        t = t.with_second(0)?;
                    }
                    t += chrono::Duration::minutes(1);

                    if t.minute() == 0 {
                        continue 'hour_loop;
                    }
                } else {
                    break 'hour_loop;
                }
            }
        }

        loop {
            if t.year() > year_limit {
                return None;
            }

            let second_bit = 1u64 << t.second() as u64;
            if second_bit & self.second == 0 {
                t += chrono::Duration::seconds(1);

                if t.second() == 0 {
                    let minute_bit = 1u64 << t.minute() as u64;
                    if minute_bit & self.minute == 0 {
                        continue;
                    }
                }
            } else {
                break;
            }
        }

        Some(t.with_timezone(&chrono::Utc))
    }

    fn day_matches<Tz: TimeZone>(&self, t: &DateTime<Tz>) -> bool {
        let dom_match = (1u64 << (t.day() as u64 - 1)) & self.dom > 0;
        let dow_match =
            (1u64 << t.weekday().num_days_from_sunday() as u64) & self.dow > 0;

        if self.dom & STAR_BIT > 0 || self.dow & STAR_BIT > 0 {
            dom_match && dow_match
        } else {
            dom_match || dow_match
        }
    }

    fn calculate_next(&self, prev: i64) -> Option<i64> {
        let secs = prev / 1_000_000_000;
        let nanos = (prev % 1_000_000_000) as u32;

        let prev_time = chrono::Utc.timestamp_opt(secs, nanos).single()?;
        let next_time = self.next_time(prev_time)?;
        let next_ns = next_time.timestamp_nanos_opt()?;
        Some(next_ns)
    }
}

#[async_trait]
impl Trigger for SpecTrigger {
    fn next_fire_time(&self, prev: i64) -> Option<i64> {
        let cached_prev = self.cache_prev.load(Ordering::SeqCst);
        if cached_prev == prev {
            let cached_next = self.cache_next.load(Ordering::SeqCst);
            if cached_next != i64::MIN {
                return Some(cached_next);
            }
        }

        let next = self.calculate_next(prev)?;

        self.cache_prev.store(prev, Ordering::SeqCst);
        self.cache_next.store(next, Ordering::SeqCst);

        Some(next)
    }

    fn description(&self) -> String {
        format!("cron:{}", self.spec)
    }
}
