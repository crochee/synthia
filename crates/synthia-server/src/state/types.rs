//! Lifecycle state enum for per-session agent runs.

/// Lifecycle state of a session's agent run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentSessionState {
    Idle,
    Running,
    Completed,
    Cancelled,
}
