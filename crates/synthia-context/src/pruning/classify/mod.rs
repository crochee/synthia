//! Message classification + tool-pair repair + `micro_compact`.
//!
//! [`classify_messages`] tags a slice of [`Message`]s with
//! [`MessageClassification`] so downstream pruning decisions know
//! which messages to preserve verbatim and which to compress.
//! [`fix_tool_pairs`] ensures every tool-use message has a matching
//! tool-result message (inserting placeholders for orphans) so the
//! provider doesn't reject the conversation.
//! [`micro_compact`] is a cheap in-place trim that keeps the most
//! recent N tool-result bodies and replaces the rest with
//! `"[cleared]"`.

mod repair;
mod types;

#[cfg(test)]
mod tests;

pub use repair::{
    find_result_for_tool_use,
    find_tool_use_for_result,
    fix_tool_pairs,
    micro_compact,
};
#[allow(unused_imports)]
pub(crate) use types::classify_message;
pub use types::{
    MessageClassification,
    classify_messages,
    get_tool_result_id,
    get_tool_use_id,
    is_tool_result,
    is_tool_use,
    is_user_text_message,
};
