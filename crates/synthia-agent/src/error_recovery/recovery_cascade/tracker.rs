//! Per-tool consecutive-failure counter for L3.
//!
//! [`ConsecutiveFailureTracker`] is a tiny
//! `HashMap<tool_name, count>` wrapper used by the L3
//! step ([`super::l3::try_l3_fallback`]) to decide
//! whether a tool has failed often enough to warrant
//! applying its registered fallback message.
//!
//! Five methods: [`new`](Self::new),
//! [`record_failure`](Self::record_failure) (returns
//! the new count so the caller can decide immediately
//! whether to fire the fallback),
//! [`record_success`](Self::record_success) (clears the
//! tool's entry on a successful invocation),
//! [`reset`](Self::reset) (wipes the entire map), and
//! [`failure_count`](Self::failure_count) (read-only
//! lookup used by tests).
//!
//! Kept separate from [`super::core`] (the enum +
//! threshold) and from the L*-step modules because the
//! tracker is plain data + five tiny methods; bundling
//! it with any single L* step would be a false
//! "belongs to step X" signal.

use std::collections::HashMap;

/// Per-tool consecutive-failure counter used by L3 to decide whether to
/// apply the fallback message.
#[derive(Debug, Default)]
pub struct ConsecutiveFailureTracker {
    failures: HashMap<String, u32>,
}

impl ConsecutiveFailureTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a failure for `tool_name` and returns the new consecutive
    /// failure count.
    pub fn record_failure(&mut self, tool_name: &str) -> u32 {
        let entry = self.failures.entry(tool_name.to_string()).or_insert(0);
        *entry += 1;
        *entry
    }

    /// Clears the failure count for `tool_name`.
    pub fn record_success(&mut self, tool_name: &str) {
        self.failures.remove(tool_name);
    }

    /// Resets the tracker entirely.
    pub fn reset(&mut self) {
        self.failures.clear();
    }

    /// Returns the current consecutive failure count for `tool_name`.
    pub fn failure_count(&self, tool_name: &str) -> u32 {
        self.failures.get(tool_name).copied().unwrap_or(0)
    }
}
