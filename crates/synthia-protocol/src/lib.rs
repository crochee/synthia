//! Wire protocol types for synthia CLI/server/IDE clients.
//!
//! Adopts codex `codex-rs/protocol` patterns:
//! - `Submission` + `Op` for client-to-agent requests
//! - `EventMsg` for agent-to-client events
//! - `W3cTraceContext` for distributed tracing
//! - `AskForApproval` + `PermissionDecision` for tool approval

#![deny(unsafe_code)]

pub mod approval;
pub mod error;
pub mod event;
pub mod id;
pub mod projection;
pub mod submission;
pub mod trace;
pub mod version;

pub use approval::{
    ApprovalRequest,
    AskForApproval,
    ExecApprovalRequirement,
    GranularApprovalConfig,
    PermissionDecision,
};
pub use error::{ProtocolError, Result as ProtocolResult};
pub use event::{CompactReason, EventMsg, TokenUsage, ToolOutput, TurnStatus};
pub use id::{ApprovalId, CallId, MessageId, SessionId, SubmissionId, TurnId};
pub use projection::project_custom_event;
pub use submission::{InputItem, Op, Submission, ThinkingLevel};
pub use trace::W3cTraceContext;
pub use version::PROTOCOL_VERSION;
