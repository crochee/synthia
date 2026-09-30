//! The wire operations a [`SessionController`](super::SessionController)
//! accepts, and the lifecycle state it reports.

/// Operations that can be submitted to a
/// [`SessionController`](super::SessionController).
#[derive(Debug, Clone)]
pub enum SessionOp {
    /// Plain-text prompt. Flows through the in-memory
    /// [`synthia::session::manager::InputQueue`] so a multi-turn
    /// chat can accumulate prompts while a run is in flight and
    /// drain them on the next `maybe_start_run`.
    Prompt {
        content: String,
        priority: u8,
    },
    /// Multimodal prompt. Carries one or more
    /// [`synthia::provider::ContentPart`] values (text + image /
    /// audio / file). Bypasses the text-only `InputQueue` —
    /// multimodality is meaningless without the original bytes,
    /// so we can't losslessly round-trip through the JSON-string
    /// `Value` channel the queue exposes. `agent_name`, when
    /// `Some`, overrides the configured default for this run
    /// Set `Some(name)` to override the configured default.
    PromptMulti {
        parts: Vec<synthia::provider::ContentPart>,
        agent_name: Option<String>,
        priority: u8,
    },
    /// Rerun a user turn from parts recovered off the durable log.
    /// Used by the `POST /api/v1/chat/sessions/:id/regenerate`
    /// endpoint that powers the chat UI's "Regenerate" affordance.
    ///
    /// The payload is whatever the log held: the prompt **text**.
    /// Attachment bytes are not persisted (see the persistence block
    /// in the run task), so a regenerated image turn replays its text
    /// without the image. That is a property of the caller, not of
    /// this op — a caller holding the bytes may pass them.
    ///
    /// `agent_name` follows the same override semantics as
    /// `PromptMulti`. `priority` mirrors `PromptMulti`'s.
    Rerun {
        parts: Vec<synthia::provider::ContentPart>,
        agent_name: Option<String>,
        priority: u8,
    },
    /// Append a feedback record to the session sink. The wire
    /// payload is `{thumbs_up: bool, message_id: String}` —
    /// persisted as a JSONL row tagged `kind: "feedback"` so a
    /// future analytics endpoint can aggregate it.
    Feedback {
        message_id: String,
        thumbs_up: bool,
    },
    Steer {
        content: String,
        priority: u8,
    },
    Cancel {
        reason: Option<String>,
    },
    Shutdown,
}

/// Lifecycle state of a session controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Idle,
    Running,
    Cancelled,
}
