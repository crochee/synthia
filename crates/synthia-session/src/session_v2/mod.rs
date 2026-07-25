//! Part-based session storage (opencode V2 message model).
//!
//! - `Message { info, parts[] }` + 11-variant `Part`
//! - `ToolPart` + 4-state `ToolState` + type-safe `ToolTime.compacted`
//! - 14-variant `SessionEntry` (one per JSONL line, append-only)
//! - `SessionTree` with `leaf` pointer + cached `paths_from_root`
//!
//! Merged from `synthia-session-v2` crate into `synthia-session::session_v2`.

pub mod branch;
pub mod entry;
pub mod error;
pub mod manager;
pub mod message;
pub mod part;
pub mod session_versions;
pub mod tool_part;
pub mod tree;
pub mod writer_task;

pub use entry::SessionEntry;
pub use error::{Result, SessionError};
pub use manager::SessionManager;
pub use message::{Message, MessageError, MessageInfo, MessageTime, Role};
pub use part::{
    AgentPart,
    CompactionPart,
    FilePart,
    Part,
    PatchPart,
    ReasoningPart,
    SnapshotPart,
    StepFinishPart,
    StepStartPart,
    SubtaskPart,
    TextPart,
};
pub use session_versions::CURRENT_SESSION_VERSION;
pub use tool_part::{AttachmentRef, ToolPart, ToolState, ToolTime};
pub use tree::{MessageKey, SessionTree};
pub use writer_task::{TreeCmd, session_writer_task};
