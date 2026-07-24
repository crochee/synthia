//! Time wheel based job scheduler

pub mod cron;
pub mod job;
pub mod time_wheel;
pub mod trigger;

pub use cron::{ParseOption, Parser, every, parse_standard, run_at, run_once};
pub use job::{Job, ScheduledJob};
pub use synthia_core::Error;
pub use time_wheel::TimeWheel;
pub use trigger::Trigger;
pub type Result<T> = std::result::Result<T, Error>;
