//! Cron expression parsing and scheduling
//!
//! This module provides a cron expression parser that matches the behavior of the Go version.

mod parser;
mod spec_trigger;

pub use parser::{ParseOption, Parser, parse_standard};
pub use spec_trigger::SpecTrigger;

use crate::JobError;
pub use crate::trigger::{every, run_at, run_once};

/// Parse a cron expression and return a boxed Trigger
///
/// # Arguments
/// * `expression` - Cron expression string
///
/// # Returns
/// A boxed Trigger on success, or a JobError on failure
pub fn parse(expression: &str) -> Result<Box<dyn crate::Trigger>, JobError> {
    parse_standard(expression)
}
