//! SpecTrigger implementation for cron-based scheduling

use std::sync::atomic::{AtomicI64, Ordering};

use async_trait::async_trait;
use chrono::{DateTime, Datelike, TimeZone, Timelike};
use chrono_tz::Tz;

use crate::trigger::Trigger;

const STAR_BIT: u64 = 1 << 63;

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

        loop {
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
            } else {
                break;
            }
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
                    let hour_bit = 1u64 << t.hour() as u64;
                    if hour_bit & self.hour == 0 {
                        continue;
                    }
                }
            } else {
                break;
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

fn add_months<Tz: TimeZone>(dt: DateTime<Tz>, months: u32) -> DateTime<Tz> {
    let year = dt.year();
    let month = dt.month();
    let day = dt.day();

    let total_months = (year * 12 + month as i32 - 1) + months as i32;
    let new_year = total_months / 12;
    let new_month = (total_months % 12 + 1) as u32;

    let max_day = days_in_month(new_year, new_month);
    let new_day = day.min(max_day);

    dt.timezone()
        .with_ymd_and_hms(
            new_year,
            new_month,
            new_day,
            dt.hour(),
            dt.minute(),
            dt.second(),
        )
        .single()
        .unwrap_or(dt)
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 31,
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    #[test]
    fn test_spec_trigger_hourly() {
        let trigger = SpecTrigger::new(CronSpec {
            second: 1,
            minute: 1,
            hour: u64::MAX >> 40,
            dom: u64::MAX >> 32 | STAR_BIT,
            month: u64::MAX >> 51 | STAR_BIT,
            dow: u64::MAX >> 57 | STAR_BIT,
            location: None,
            spec: "@hourly".to_string(),
        });

        let now = Utc::now();
        let now_ns = now.timestamp_nanos_opt().unwrap();

        let next_ns = trigger.next_fire_time(now_ns).unwrap();
        let next_time = chrono::Utc.timestamp_nanos(next_ns);

        let diff = next_time - now;
        assert!(diff.num_seconds() > 0);
        assert!(diff.num_seconds() <= 3600);
    }

    #[test]
    fn test_spec_trigger_daily() {
        let trigger = SpecTrigger::new(CronSpec {
            second: 1,
            minute: 1,
            hour: 1,
            dom: u64::MAX >> 32 | STAR_BIT,
            month: u64::MAX >> 51 | STAR_BIT,
            dow: u64::MAX >> 57 | STAR_BIT,
            location: None,
            spec: "@daily".to_string(),
        });

        let now = Utc::now();
        let now_ns = now.timestamp_nanos_opt().unwrap();

        let next_ns = trigger.next_fire_time(now_ns).unwrap();
        let next_time = chrono::Utc.timestamp_nanos(next_ns);

        assert_eq!(next_time.hour(), 0);
        assert_eq!(next_time.minute(), 0);
        assert_eq!(next_time.second(), 0);
    }

    #[test]
    fn test_spec_trigger_description() {
        let trigger = SpecTrigger::new(CronSpec {
            second: 1,
            minute: 1,
            hour: 1,
            dom: u64::MAX,
            month: u64::MAX,
            dow: u64::MAX,
            location: None,
            spec: "0 0 * * *".to_string(),
        });

        assert_eq!(trigger.description(), "cron:0 0 * * *");
    }

    #[test]
    fn test_spec_trigger_cache() {
        let trigger = SpecTrigger::new(CronSpec {
            second: 1,
            minute: 1,
            hour: 1,
            dom: u64::MAX >> 32 | STAR_BIT,
            month: u64::MAX >> 51 | STAR_BIT,
            dow: u64::MAX >> 57 | STAR_BIT,
            location: None,
            spec: "@daily".to_string(),
        });

        let now = Utc::now();
        let now_ns = now.timestamp_nanos_opt().unwrap();

        let next1 = trigger.next_fire_time(now_ns).unwrap();
        let next2 = trigger.next_fire_time(now_ns).unwrap();

        assert_eq!(next1, next2);
    }

    #[test]
    fn test_days_in_month() {
        assert_eq!(days_in_month(2024, 1), 31);
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2023, 2), 28);
        assert_eq!(days_in_month(2024, 4), 30);
    }
}
