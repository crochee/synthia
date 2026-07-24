use chrono::{TimeZone, Timelike, Utc};

use super::{
    calendar::days_in_month,
    spec::{CronSpec, STAR_BIT, SpecTrigger},
};
use crate::trigger::Trigger;

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
