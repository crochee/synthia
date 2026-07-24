//! Recovery helpers for resumed sessions.
//!
//! When a session resumes from a checkpoint, any in-flight tool calls
//! observed in the saved message stream need to be marked so the runtime
//! can either re-execute or clean them up. [`patch_tool_calls_recovery`]
//! inserts a `"status": "executing"` marker on every assistant
//! [`synthia_provider::ContentPart::ToolUse`] whose `input` does not
//! already carry a `"status"` field.

use synthia_provider::{
    Content,
    ContentPart,
    types::{Message, Role},
};

/// Patch unfinished tool calls recovered from a checkpoint.
///
/// Marks tool calls whose input lacks a `"status"` field as `"executing"`
/// so the recovery flow can re-execute or clean them up.
pub fn patch_tool_calls_recovery(messages: &mut [Message]) -> usize {
    let mut patched = 0;
    for msg in messages.iter_mut() {
        if matches!(msg.role, Role::Assistant)
            && let Content::Single(ContentPart::ToolUse(tu)) = &mut msg.content
            && tu.input.as_object().and_then(|o| o.get("status")).is_none()
        {
            if let Some(obj) = tu.input.as_object_mut() {
                obj.insert(
                    "status".to_string(),
                    serde_json::json!("executing"),
                );
            } else {
                tu.input = serde_json::json!({"status": "executing"});
            }
            patched += 1;
        }
    }
    patched
}
