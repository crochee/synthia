//! Time wheel implementation for efficient job scheduling
//!
//! This module provides a time wheel based scheduler that efficiently manages
//! large numbers of scheduled jobs with O(1) add/remove operations.

mod entry;
mod wheel;

pub use wheel::TimeWheel;
