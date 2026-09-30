//! History commit + final-message extraction.
//!
//! Tiny two-helper module. Kept apart from the dispatch
//! pipeline because both [`super::bucket`]
//! and [`super::steps`] (the length-stop failure path)
//! need to append a tool result to the live history, and
//! because the final-message projection is what
//! [`super::steps::finalize`] hands back as the
//! session's user-visible text.
//!
//! The wire payload carried in is [`super::dispatch::WireToolResult`]
//! — a small struct defined alongside the dispatch seam
//! because that's the seam that produces it.

use synthia_provider::{Content, ContentPart, Message, Role, ToolResult};

use super::{ReActLoop, dispatch::WireToolResult};

/// Append one tool's result to the live history and surface
/// the result on the model's event channel.
///
/// The history append and the event emission are kept in one
/// fn so callers cannot forget one half — a tool result the
/// model sees on the channel but not in history would lead
/// to a corrupted transcript on the next LLM pass.
pub(super) fn commit_tool_result(
    this: &ReActLoop,
    messages: &mut Vec<Message>,
    tr: WireToolResult,
) {
    // `structured_output` validates before returning and marks a
    // violation `is_error`, so a non-error result is exactly a
    // schema-valid submission. This is the one funnel every tool result
    // passes through, so it is the one place the fact can be recorded
    // for the run's finalize to read.
    if tr.tool_name == synthia_tool::STRUCTURED_OUTPUT_TOOL_NAME && !tr.is_error
    {
        *this.structured_output.lock() = true;
    }
    messages.push(Message::tool(
        Content::parts(tr.content.clone()),
        tr.call_id.clone(),
    ));
    this.sink.model(ContentPart::ToolResult(ToolResult {
        tool_use_id: tr.call_id,
        tool_name: Some(tr.tool_name),
        content: tr.content,
        structured_content: None,
        is_error: Some(tr.is_error),
        metadata: tr.metadata,
        truncated_by: tr.truncated_by,
    }));
}

/// Pull the last assistant turn's text out of `messages` for the
/// final-message output.
pub(super) fn last_assistant_text(messages: &[Message]) -> Option<String> {
    messages
        .iter()
        .rev()
        .find(|m| matches!(m.role, Role::Assistant))
        .and_then(|m| m.content.extract_text())
}
