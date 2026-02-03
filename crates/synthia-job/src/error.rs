//! Error types for job scheduling

use std::fmt;

#[derive(Debug, Clone)]
pub enum JobError {
    DuplicateJob(String),
    JobNotFound(String),
    DuplicateRun,
    ParseError(String),
    CronParseError(String),
    InvalidTimestamp(String),
    SchedulerNotRunning,
    ChannelError(String),
    NoRuntime(String),
}

impl std::error::Error for JobError {}

impl fmt::Display for JobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateJob(id) => {
                write!(f, "Job with ID '{id}' already exists")
            }
            Self::JobNotFound(id) => write!(f, "Job with ID '{id}' not found"),
            Self::DuplicateRun => write!(f, "Scheduler is already running"),
            Self::ParseError(msg) => write!(f, "Parse error: {msg}"),
            Self::CronParseError(msg) => {
                write!(f, "Failed to parse cron expression: {msg}")
            }
            Self::InvalidTimestamp(msg) => {
                write!(f, "Invalid timestamp: {msg}")
            }
            Self::SchedulerNotRunning => {
                write!(f, "Scheduler is not running")
            }
            Self::ChannelError(msg) => {
                write!(f, "Channel error: {msg}")
            }
            Self::NoRuntime(msg) => {
                write!(f, "No async runtime available: {msg}")
            }
        }
    }
}

impl From<std::io::Error> for JobError {
    fn from(err: std::io::Error) -> Self {
        JobError::ParseError(err.to_string())
    }
}
