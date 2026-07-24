//! 3 reason / status enums + 2 helper event structs:
//!
//! - [`SessionEndReason`] — why a session ended.
//! - [`TurnEndReason`] — why a turn ended.
//! - [`AgentStatus`] — current status of an agent.
//! - [`ErrorSource`] — source of an error in the agent
//!   lifecycle.
//! - [`ProgressEvent`] — long-running-operation progress
//!   event.
//! - [`ErrorEvent`] — error event payload.
//!
//! All enum types derive `Clone + Debug + Serialize +
//! Deserialize` (with `PartialEq` where equality is
//! meaningful).

use serde::{Deserialize, Serialize};

/// Reason why a session ended.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum SessionEndReason {
    Completed,
    Cancelled,
    Error(String),
    TokenBudgetExceeded,
    MaxIterationsReached,
    GuardianBlocked,
    LoopDetected,
    CircuitBreakerOpen,
}

/// Source of an error in the agent lifecycle.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum ErrorSource {
    Llm,
    Tool(String),
    Hook(String),
    Internal,
    Configuration,
}

/// Error event containing error information.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ErrorEvent {
    pub source: ErrorSource,
    pub message: String,
    pub recoverable: bool,
}

/// Reason why a turn ended.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum TurnEndReason {
    Success,
    Error(String),
    Cancelled,
    MaxStepsReached,
    TokenBudgetExceeded,
}

/// Progress event for tracking long-running operations.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProgressEvent {
    pub operation: String,
    pub current: usize,
    pub total: usize,
    pub message: Option<String>,
}

/// Current status of an agent.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum AgentStatus {
    PendingInit,
    Running,
    Completed,
    Errored(String),
    Shutdown,
    Cancelled,
    MaxStepsReached(u32),
    NotFound,
    LoopDetected(String),
    MaxTokensReached(u64),
}
