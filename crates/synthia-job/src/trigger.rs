//! Trigger types for job scheduling
//!
//! This module provides various trigger implementations that determine when jobs should be executed.

use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

pub trait Trigger: Send + Sync {
    fn next_fire_time(&self, prev: i64) -> Option<i64>;
    fn description(&self) -> String;
}

fn format_duration_ns(ns: i64) -> String {
    let secs = ns / 1_000_000_000;
    let millis = (ns % 1_000_000_000) / 1_000_000;
    if millis == 0 {
        format!("{secs}s")
    } else {
        format!("{secs}.{millis:03}s")
    }
}

#[derive(Debug, Clone)]
pub struct ConstantDelayTrigger {
    interval_ns: i64,
}

impl ConstantDelayTrigger {
    pub fn new(interval_ns: i64) -> Self {
        Self { interval_ns }
    }

    pub fn from_duration(interval: Duration) -> Self {
        Self {
            interval_ns: interval.as_nanos() as i64,
        }
    }
}

impl Trigger for ConstantDelayTrigger {
    fn next_fire_time(&self, prev: i64) -> Option<i64> {
        Some(prev + self.interval_ns)
    }

    fn description(&self) -> String {
        format!("Every {}", format_duration_ns(self.interval_ns))
    }
}

pub fn every(interval: Duration) -> ConstantDelayTrigger {
    ConstantDelayTrigger::from_duration(interval)
}

#[derive(Debug)]
pub struct RunAtTrigger {
    at_ns: i64,
    expired: AtomicBool,
}

impl RunAtTrigger {
    pub fn new(at_ns: i64) -> Self {
        Self {
            at_ns,
            expired: AtomicBool::new(false),
        }
    }
}

impl Trigger for RunAtTrigger {
    fn next_fire_time(&self, prev: i64) -> Option<i64> {
        if self.expired.swap(true, Ordering::SeqCst) {
            None
        } else {
            Some(prev.max(self.at_ns))
        }
    }

    fn description(&self) -> String {
        let secs = self.at_ns / 1_000_000_000;
        format!("Run at timestamp {secs}s")
    }
}

pub fn run_at(at: i64) -> RunAtTrigger {
    RunAtTrigger::new(at)
}

#[derive(Debug)]
pub struct RunOnceTrigger {
    delay_ns: i64,
    expired: AtomicBool,
}

impl RunOnceTrigger {
    pub fn new(delay_ns: i64) -> Self {
        Self {
            delay_ns,
            expired: AtomicBool::new(false),
        }
    }

    pub fn from_duration(delay: Duration) -> Self {
        Self {
            delay_ns: delay.as_nanos() as i64,
            expired: AtomicBool::new(false),
        }
    }
}

impl Trigger for RunOnceTrigger {
    fn next_fire_time(&self, prev: i64) -> Option<i64> {
        if self.expired.swap(true, Ordering::SeqCst) {
            None
        } else {
            Some(prev + self.delay_ns)
        }
    }

    fn description(&self) -> String {
        format!("Run once after {}", format_duration_ns(self.delay_ns))
    }
}

pub fn run_once(delay: Duration) -> RunOnceTrigger {
    RunOnceTrigger::from_duration(delay)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trigger_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<ConstantDelayTrigger>();
        assert_send_sync::<RunOnceTrigger>();
        assert_send_sync::<RunAtTrigger>();
    }

    #[test]
    fn test_format_duration_ns() {
        assert_eq!(format_duration_ns(0), "0s");
        assert_eq!(format_duration_ns(1_000_000_000), "1s");
        assert_eq!(format_duration_ns(1_500_000_000), "1.500s");
        assert_eq!(format_duration_ns(10_000_000_000), "10s");
    }

    #[test]
    fn test_constant_delay_trigger() {
        let trigger = every(Duration::from_secs(10));
        assert_eq!(trigger.next_fire_time(100), Some(100 + 10_000_000_000));
        assert_eq!(trigger.next_fire_time(200), Some(200 + 10_000_000_000));
        assert!(trigger.description().contains("Every"));
    }

    #[test]
    fn test_run_at_trigger() {
        let target_time = 1000i64;
        let trigger = run_at(target_time);

        assert_eq!(trigger.next_fire_time(500), Some(target_time));

        assert_eq!(trigger.next_fire_time(600), None);
    }

    #[test]
    fn test_run_at_trigger_with_later_time() {
        let target_time = 1000i64;
        let trigger = run_at(target_time);

        assert_eq!(trigger.next_fire_time(2000), Some(2000));
    }

    #[test]
    fn test_run_once_trigger() {
        let trigger = run_once(Duration::from_secs(5));
        assert_eq!(trigger.next_fire_time(100), Some(100 + 5_000_000_000));

        assert_eq!(trigger.next_fire_time(200), None);

        assert!(trigger.description().contains("Run once"));
    }
}
